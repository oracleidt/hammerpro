mod canvas;
mod inspector;
mod labels;
mod preview;
#[cfg(test)]
mod tests;
pub mod tr;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use egui::{Align2, Color32, Key, Modifiers, Rect, Sense, Ui, pos2, vec2};
use hp_app::runtime::ShaderRequest;
use hp_app::{AppState, Severity, t, tf, theme};
use hp_shader::{BakeOptions, BakeReport, Diagnostic, FileLoader, Graph, ImageLoader, Probe};

use self::canvas::{Canvas, View};
pub use self::preview::EditorLoader;
use self::preview::{Channel, Params, Preview};
use crate::util::PendingPick;

const PREVIEW_RES: [u32; 3] = [128, 256, 512];
const BAKE_SIZES: [u32; 7] = [0, 64, 128, 256, 512, 1024, 2048];
const SETTLE: Duration = Duration::from_millis(320);
const MAX_UNDO: usize = 100;

struct Doc {
    graph: Graph,
    committed: Graph,
    previous: Graph,
    saved: Graph,
    undo: Vec<Graph>,
    redo: Vec<Graph>,
    pending_since: Option<Instant>,
}

impl Doc {
    fn new(graph: Graph) -> Self {
        Self {
            committed: graph.clone(),
            previous: graph.clone(),
            saved: graph.clone(),
            graph,
            undo: Vec::new(),
            redo: Vec::new(),
            pending_since: None,
        }
    }

    fn reset(&mut self, graph: Graph) {
        *self = Self::new(graph);
    }

    fn load(&mut self, graph: Graph) {
        if !self.dirty() {
            self.reset(graph);
            return;
        }
        self.commit();
        self.undo.push(std::mem::replace(&mut self.committed, graph.clone()));
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.previous = graph.clone();
        self.saved = graph.clone();
        self.graph = graph;
        self.pending_since = None;
    }

    fn dirty(&self) -> bool {
        self.graph != self.saved
    }

    fn commit(&mut self) {
        if self.graph == self.committed {
            self.pending_since = None;
            return;
        }
        self.undo.push(std::mem::replace(&mut self.committed, self.graph.clone()));
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.pending_since = None;
    }

    fn settle(&mut self, pointer_down: bool) -> bool {
        if self.graph != self.previous {
            self.previous = self.graph.clone();
            self.pending_since = Some(Instant::now());
        }
        if let Some(t) = self.pending_since
            && !pointer_down
            && t.elapsed() >= SETTLE
        {
            self.commit();
        }
        self.pending_since.is_some()
    }

    fn undo(&mut self) -> bool {
        self.commit();
        let Some(prev) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.committed, prev.clone()));
        self.graph = prev.clone();
        self.previous = prev;
        true
    }

    fn redo(&mut self) -> bool {
        self.commit();
        let Some(next) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.committed, next.clone()));
        self.graph = next.clone();
        self.previous = next;
        true
    }

    fn replace(&mut self, graph: Graph) {
        self.commit();
        self.graph = graph;
    }
}

struct Export {
    dest: usize,
    name: String,
    surface_prop: String,
    overwrite: bool,
    save_graph: bool,
    lines: Vec<String>,
    error: Option<String>,
}

impl Default for Export {
    fn default() -> Self {
        Self {
            dest: 0,
            name: String::new(),
            surface_prop: String::new(),
            overwrite: false,
            save_graph: true,
            lines: Vec::new(),
            error: None,
        }
    }
}

struct BakeJob {
    rx: Receiver<Result<BakeReport, String>>,
    dest_dir: PathBuf,
    stem: String,
    graph: Graph,
}

enum Pending {
    Open(PendingPick<PathBuf>),
    Save(PendingPick<PathBuf>),
    Image(u32, PendingPick<PathBuf>),
}

pub struct ShaderEditor {
    doc: Doc,
    path: Option<PathBuf>,
    image_dir: Option<PathBuf>,
    canvas: Canvas,
    preview: Preview,
    probe: Option<Probe>,
    channel: Channel,
    tile: u32,
    res: u32,
    node_previews: bool,
    loader: Arc<EditorLoader>,
    loader_dirs: Vec<PathBuf>,
    export: Export,
    pending: Option<Pending>,
    bake: Option<BakeJob>,
    diagnostics: Vec<Diagnostic>,
    interactive: bool,
    used: bool,
    mirrored: Option<Graph>,
    mirror_json: String,
}

impl Default for ShaderEditor {
    fn default() -> Self {
        Self {
            doc: Doc::new(Graph::new()),
            path: None,
            image_dir: None,
            canvas: Canvas::default(),
            preview: Preview::default(),
            probe: None,
            channel: Channel::Color,
            tile: 1,
            res: 256,
            node_previews: true,
            loader: Arc::new(EditorLoader { files: FileLoader::default(), fs: None }),
            loader_dirs: Vec::new(),
            export: Export::default(),
            pending: None,
            bake: None,
            diagnostics: Vec::new(),
            interactive: false,
            used: false,
            mirrored: None,
            mirror_json: String::new(),
        }
    }
}

fn file_leaf(source: &str) -> String {
    let leaf = source.rsplit(['/', '\\']).next().unwrap_or(source);
    let stem = leaf.rsplit_once('.').map_or(leaf, |(s, _)| s);
    hp_shader::sanitize_stem(stem)
}

impl ShaderEditor {
    pub fn graph(&self) -> &Graph {
        &self.doc.graph
    }

    pub fn apply_ops(&mut self, ops: &[serde_json::Value]) -> Result<Vec<serde_json::Value>, String> {
        let result = hp_shader::apply_ops(&mut self.doc.graph, ops)?;
        let g = &self.doc.graph;
        self.canvas.selected.retain(|id| g.node(*id).is_some());
        Ok(result)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn load_graph(&mut self, graph: Graph, path: Option<PathBuf>, name: &str) {
        self.doc.load(graph);
        self.path = path;
        self.probe = None;
        self.canvas.selected.clear();
        self.canvas.want_fit = true;
        self.export.lines.clear();
        self.export.error = None;
        if !name.is_empty() {
            self.export.name = name.to_owned();
        }
        self.preview.invalidate();
    }

    pub fn open_request(&mut self, state: &mut AppState, req: ShaderRequest) {
        let preset_id = req.preset.clone().unwrap_or_else(|| "blank".to_owned());
        if let Some(path) = &req.path {
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case(hp_shader::EXTENSION)) {
                self.open_file(state, path);
                return;
            }
            self.open_source(state, &path.to_string_lossy(), &preset_id);
            return;
        }
        if let Some(json) = &req.graph_json {
            match Graph::parse(json) {
                Ok((graph, warnings)) => {
                    let name = req.name.as_deref().map(hp_shader::sanitize_stem).unwrap_or_default();
                    self.load_graph(graph, None, &name);
                    self.image_dir = None;
                    for w in warnings {
                        state.log(Severity::Warning, w);
                    }
                }
                Err(e) => state.notify(Severity::Error, tf!("shader.open_failed", e)),
            }
            return;
        }
        if let Some(source) = &req.source {
            self.open_source(state, source, &preset_id);
            return;
        }
        if let Some(preset) = &req.preset
            && let Some(g) = hp_shader::preset(preset, "")
        {
            self.load_graph(g, None, "");
        }
    }

    fn open_file(&mut self, state: &mut AppState, path: &Path) {
        match hp_shader::load_file(path) {
            Ok((graph, warnings)) => {
                let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                self.load_graph(graph, Some(path.to_path_buf()), &hp_shader::sanitize_stem(&name));
                self.image_dir = None;
                if !warnings.is_empty() {
                    state.notify(Severity::Warning, tf!("shader.loaded_repaired", warnings.len()));
                    for w in warnings {
                        state.log(Severity::Warning, w);
                    }
                }
            }
            Err(e) => state.notify(Severity::Error, tf!("shader.open_failed", e)),
        }
    }

    fn open_source(&mut self, state: &mut AppState, source: &str, preset_id: &str) {
        let as_path = PathBuf::from(source);
        if as_path.is_file() {
            let companion = hp_shader::companion_path(&as_path);
            if companion.is_file() {
                self.open_file(state, &companion);
                self.image_dir = as_path.parent().map(Path::to_path_buf);
                return;
            }
            let name = file_leaf(source);
            let graph = hp_shader::preset(preset_id, source).unwrap_or_default();
            self.load_graph(graph, None, &name);
            self.image_dir = as_path.parent().map(Path::to_path_buf);
            return;
        }
        let material = crate::util::normalize_material(source);
        if let Some(file) = crate::browser::material_disk_file(state, &material) {
            let companion = hp_shader::companion_path(&file);
            if companion.is_file() {
                self.open_file(state, &companion);
                return;
            }
        }
        let texture = state
            .assets
            .material_info(&material)
            .and_then(|i| i.base_texture.clone())
            .filter(|t| !t.trim().is_empty())
            .unwrap_or(material.clone());
        let graph = hp_shader::preset(preset_id, &texture).unwrap_or_default();
        let leaf = crate::util::leaf_name(&material);
        self.load_graph(graph, None, &format!("{}_edit", hp_shader::sanitize_stem(leaf)));
        self.image_dir = None;
    }

    fn refresh_loader(&mut self, state: &AppState) {
        let mut dirs: Vec<PathBuf> = Vec::new();
        let mut push = |d: Option<&Path>| {
            if let Some(d) = d
                && !dirs.iter().any(|x| x == d)
            {
                dirs.push(d.to_path_buf());
            }
        };
        push(self.path.as_deref().and_then(Path::parent));
        push(self.image_dir.as_deref());
        push(state.doc.path().and_then(Path::parent));
        let fs = state.assets.fs().cloned();
        let had_fs = self.loader.fs.is_some();
        if dirs != self.loader_dirs || had_fs != fs.is_some() {
            self.loader = Arc::new(EditorLoader { files: FileLoader::new(dirs.clone()), fs });
            self.loader_dirs = dirs;
            self.preview.invalidate();
        }
    }

    fn poll_dialogs(&mut self, state: &mut AppState) {
        let Some(p) = self.pending.take() else { return };
        match p {
            Pending::Open(pick) => match pick.poll() {
                None => self.pending = Some(Pending::Open(pick)),
                Some(Some(path)) => self.open_file(state, &path),
                Some(None) => {}
            },
            Pending::Save(pick) => match pick.poll() {
                None => self.pending = Some(Pending::Save(pick)),
                Some(Some(path)) => self.save_to(state, path),
                Some(None) => {}
            },
            Pending::Image(node, pick) => match pick.poll() {
                None => self.pending = Some(Pending::Image(node, pick)),
                Some(Some(path)) => {
                    let value = path.to_string_lossy().into_owned();
                    let _ = self.doc.graph.set_param(node, "source", serde_json::json!(value));
                    if self.export.name.is_empty() {
                        self.export.name = file_leaf(&value);
                    }
                    self.image_dir = path.parent().map(Path::to_path_buf);
                }
                Some(None) => {}
            },
        }
    }

    fn save_to(&mut self, state: &mut AppState, path: PathBuf) {
        let path = if path.extension().is_some() { path } else { path.with_extension(hp_shader::EXTENSION) };
        let mut g = self.doc.graph.clone();
        if let Some(dir) = path.parent() {
            hp_shader::relativize_sources(&mut g, dir);
        }
        match hp_shader::save_file(&path, &g) {
            Ok(()) => {
                self.doc.graph = g.clone();
                self.doc.saved = g;
                state.notify(Severity::Success, tf!("shader.saved", path.display()));
                self.path = Some(path);
            }
            Err(e) => state.notify(Severity::Error, tf!("shader.save_failed", e)),
        }
    }

    fn ask_save_as(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let start = self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        let name = format!("{}.{}", self.default_name(), hp_shader::EXTENSION);
        let title = t!("shader.save_title").to_owned();
        self.pending = Some(Pending::Save(PendingPick::spawn(ctx, move || {
            let mut d = rfd::FileDialog::new()
                .set_title(title)
                .set_file_name(name)
                .add_filter("Hammer Pro shader", &[hp_shader::EXTENSION]);
            if let Some(s) = start {
                d = d.set_directory(s);
            }
            d.save_file()
        })));
    }

    fn ask_open(&mut self, ctx: &egui::Context) {
        if self.pending.is_some() {
            return;
        }
        let title = t!("shader.open_title").to_owned();
        let start = self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf);
        self.pending = Some(Pending::Open(PendingPick::spawn(ctx, move || {
            let mut d =
                rfd::FileDialog::new().set_title(title).add_filter("Hammer Pro shader", &[hp_shader::EXTENSION]);
            if let Some(s) = start {
                d = d.set_directory(s);
            }
            d.pick_file()
        })));
    }

    fn ask_image(&mut self, ctx: &egui::Context, node: u32) {
        if self.pending.is_some() {
            return;
        }
        let title = t!("shader.image_title").to_owned();
        let start =
            self.image_dir.clone().or_else(|| self.path.as_deref().and_then(Path::parent).map(Path::to_path_buf));
        self.pending = Some(Pending::Image(
            node,
            PendingPick::spawn(ctx, move || {
                let mut d = rfd::FileDialog::new()
                    .set_title(title)
                    .add_filter("Images", &["png", "jpg", "jpeg", "tga", "bmp", "vtf"]);
                if let Some(s) = start {
                    d = d.set_directory(s);
                }
                d.pick_file()
            }),
        ));
    }

    fn default_name(&self) -> String {
        if !self.export.name.trim().is_empty() {
            return hp_shader::sanitize_stem(&self.export.name);
        }
        self.path
            .as_deref()
            .and_then(Path::file_stem)
            .map(|s| hp_shader::sanitize_stem(&s.to_string_lossy()))
            .unwrap_or_else(|| "shader".to_owned())
    }

    fn first_source(&self) -> String {
        self.doc.graph.sources().into_iter().next().unwrap_or_default()
    }

    fn start_bake(&mut self, state: &mut AppState) {
        if self.bake.is_some() {
            return;
        }
        let Some(dir) = state.settings.game.custom_texture_dirs.get(self.export.dest) else { return };
        let layout = crate::folders::layout_of(state, dir);
        let (dest_dir, prefix) = crate::folders::import_target(&dir.path, &layout);
        let stem = hp_shader::sanitize_stem(&self.export.name);
        let sp = self.export.surface_prop.trim();
        let opts = BakeOptions {
            dest_dir: dest_dir.clone(),
            prefix,
            stem: stem.clone(),
            surface_prop: (!sp.is_empty()).then(|| sp.to_owned()),
            overwrite: self.export.overwrite,
            width: 0,
            height: 0,
        };
        let graph = self.doc.graph.clone();
        let (job_graph, loader, cache) = (graph.clone(), self.loader.clone(), self.preview.cache.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new().name("hp-shader-bake".to_owned()).spawn(move || {
            let loader: &dyn ImageLoader = loader.as_ref();
            let _ = tx.send(hp_shader::bake(&job_graph, loader, &cache, &opts));
        });
        match spawned {
            Ok(_) => {
                self.export.error = None;
                self.export.lines.clear();
                self.bake = Some(BakeJob { rx, dest_dir, stem, graph });
            }
            Err(e) => self.export.error = Some(e.to_string()),
        }
    }

    fn poll_bake(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let Some(job) = &self.bake else { return };
        let result = match job.rx.try_recv() {
            Ok(r) => r,
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(60));
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => Err("the writer stopped".to_owned()),
        };
        let Some(job) = self.bake.take() else { return };
        match result {
            Ok(report) => {
                self.export.lines = report
                    .files
                    .iter()
                    .map(|f| format!("{}  ({}x{}, {} KB)", f.texture, f.width, f.height, f.bytes.div_ceil(1024)))
                    .collect();
                if let Some((name, _)) = &report.material {
                    self.export.lines.push(format!("{name}.vmt"));
                    state.use_material(name);
                } else if let Some(f) = report.files.first() {
                    state.use_material(&f.texture);
                }
                state.assets.remount(&state.settings);
                state.notify(Severity::Success, tf!("shader.baked", report.files.len()));
                if self.export.save_graph {
                    let path = job.dest_dir.join(format!("{}.{}", job.stem, hp_shader::EXTENSION));
                    let mut g = job.graph.clone();
                    hp_shader::relativize_sources(&mut g, &job.dest_dir);
                    match hp_shader::save_file(&path, &g) {
                        Ok(()) => {
                            self.export
                                .lines
                                .push(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                            self.doc.saved = job.graph;
                            self.path = Some(path);
                        }
                        Err(e) => state.notify(Severity::Warning, tf!("shader.save_failed", e)),
                    }
                }
                self.preview.invalidate();
            }
            Err(e) => {
                state.notify(Severity::Error, tf!("shader.bake_failed", e));
                self.export.error = Some(e);
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if let Some(req) = state.runtime.shader.request.take() {
            self.open_request(state, req);
        }
        if state.runtime.shader.show {
            self.used = true;
        }
        if self.used {
            self.mirror(state);
        }
        if !state.runtime.shader.show {
            return;
        }
        self.poll_dialogs(state);
        self.poll_bake(ctx, state);
        self.refresh_loader(state);
        let pointer_down = ctx.input(|i| i.pointer.any_down());
        self.interactive = pointer_down;
        if self.doc.settle(pointer_down) {
            ctx.request_repaint_after(SETTLE);
        }
        let probe = self.probe.clone().filter(|p| self.doc.graph.node(p.node).is_some());
        self.probe = probe;
        self.preview.update(
            ctx,
            &Params {
                graph: &self.doc.graph,
                probe: self.probe.as_ref(),
                res: if pointer_down { self.res.min(128) } else { self.res },
                thumbs: self.node_previews,
                channel: self.channel,
                loader: self.loader.clone(),
            },
        );
        self.refresh_diagnostics();

        let mut open = true;
        let screen = ctx.content_rect();
        let size = vec2((screen.width() * 0.9).min(1320.0), (screen.height() * 0.88).min(820.0));
        let title = {
            let name = self.path.as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned());
            let mut s = t!("shader.title").to_owned();
            if let Some(n) = name {
                s.push_str(" - ");
                s.push_str(&n);
            }
            if self.doc.dirty() {
                s.push_str(" *");
            }
            s
        };
        hp_app::chrome::Window::new(title)
            .id(egui::Id::unique("hp_shader_editor"))
            .open(&mut open)
            .resizable(true)
            .collapsible(false)
            .default_size(size)
            .min_size(vec2(780.0, 460.0))
            .show(ctx, |ui| self.ui(ui, state));
        if !open {
            state.runtime.shader.show = false;
        }
    }

    fn mirror(&mut self, state: &mut AppState) {
        let dirty = self.doc.dirty();
        let changed = self.mirrored.as_ref() != Some(&self.doc.graph);
        if changed {
            self.mirrored = Some(self.doc.graph.clone());
            self.mirror_json = self.doc.graph.to_json();
        }
        let live = &mut state.runtime.shader.live;
        let stale = changed || live.as_ref().is_none_or(|l| l.path != self.path || l.dirty != dirty);
        if stale {
            *live = Some(hp_app::runtime::LiveGraph { json: self.mirror_json.clone(), path: self.path.clone(), dirty });
        }
    }

    fn refresh_diagnostics(&mut self) {
        let mut list = self.doc.graph.validate();
        if let Some(r) = &self.preview.result {
            for d in &r.diagnostics {
                if !list.iter().any(|x| x.node == d.node && x.code == d.code) {
                    list.push(d.clone());
                }
            }
        }
        list.retain(|d| d.node.is_none_or(|n| self.doc.graph.node(n).is_some()));
        list.sort_by_key(|d| std::cmp::Reverse(d.severity));
        self.diagnostics = list;
    }

    fn keys(&mut self, ctx: &egui::Context, window_hover: bool) {
        if !window_hover || ctx.text_edit_focused() {
            return;
        }
        let consume = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if consume(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z) || consume(Modifiers::COMMAND, Key::Y) {
            self.do_redo();
        } else if consume(Modifiers::COMMAND, Key::Z) {
            self.do_undo();
        }
    }

    fn do_undo(&mut self) {
        if self.doc.undo() {
            self.after_history();
        }
    }

    fn do_redo(&mut self) {
        if self.doc.redo() {
            self.after_history();
        }
    }

    fn after_history(&mut self) {
        let g = &self.doc.graph;
        self.canvas.selected.retain(|id| g.node(*id).is_some());
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        let window_hover = ui.rect_contains_pointer(ui.max_rect());
        self.toolbar(ui, state);
        ui.add_space(4.0);
        let total = ui.available_rect_before_wrap();
        let side_w = 350.0_f32.min(total.width() * 0.42);
        let canvas_rect = Rect::from_min_max(total.min, pos2(total.max.x - side_w - 8.0, total.max.y));
        let side_rect = Rect::from_min_max(pos2(canvas_rect.max.x + 8.0, total.min.y), total.max);
        ui.allocate_rect(total, Sense::hover());
        {
            let view = View {
                thumbs: &self.preview.thumbs,
                show_thumbs: self.node_previews,
                diagnostics: &self.diagnostics,
                probe: self.probe.as_ref(),
            };
            self.canvas.show(ui, canvas_rect, &mut self.doc.graph, &view);
        }
        if let Some((node, output)) = self.canvas.preview.take() {
            self.probe = Some(Probe { node, output });
        }
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(side_rect));
        child.set_clip_rect(side_rect);
        egui::ScrollArea::vertical().id_salt("hp_shader_side").auto_shrink([false, false]).show(&mut child, |ui| {
            ui.set_width(side_rect.width() - 12.0);
            self.side_panel(ui, state);
        });
        self.keys(&ctx, window_hover);
    }

    fn toolbar(&mut self, ui: &mut Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        ui.horizontal_wrapped(|ui| {
            ui.menu_button(t!("shader.tb.new"), |ui| {
                let source = self.first_source();
                for p in hp_shader::PRESETS {
                    if ui.button(labels::preset(p.id)).on_hover_text(labels::preset_tip(p.id)).clicked() {
                        if let Some(g) = hp_shader::preset(p.id, &source) {
                            self.doc.replace(g);
                            self.canvas.selected.clear();
                            self.canvas.want_fit = true;
                            self.probe = None;
                        }
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(labels::preset("empty")).clicked() {
                    self.doc.replace(Graph::new());
                    self.canvas.selected.clear();
                    self.probe = None;
                    ui.close();
                }
            });
            if ui.button(t!("shader.tb.open")).clicked() {
                self.ask_open(&ctx);
            }
            if ui.add_enabled(self.path.is_some(), egui::Button::new(t!("shader.tb.save"))).clicked()
                && let Some(p) = self.path.clone()
            {
                self.save_to(state, p);
            }
            if ui.button(t!("shader.tb.save_as")).clicked() {
                self.ask_save_as(&ctx);
            }
            ui.separator();
            if ui
                .add_enabled(
                    !self.doc.undo.is_empty() || self.doc.graph != self.doc.committed,
                    egui::Button::new(t!("shader.tb.undo")),
                )
                .clicked()
            {
                self.do_undo();
            }
            if ui.add_enabled(!self.doc.redo.is_empty(), egui::Button::new(t!("shader.tb.redo"))).clicked() {
                self.do_redo();
            }
            ui.separator();
            if ui.button(t!("shader.tb.add")).on_hover_text(t!("shader.tb.add_tip")).clicked() {
                self.canvas.open_library_center();
            }
            if ui.button(t!("shader.tb.fit")).on_hover_text(t!("shader.tb.fit_tip")).clicked() {
                self.canvas.want_fit = true;
            }
            if ui.button(t!("shader.tb.reload")).on_hover_text(t!("shader.tb.reload_tip")).clicked() {
                self.preview.invalidate();
            }
            ui.separator();
            ui.label(format!("{:.0}%", self.canvas.zoom * 100.0));
            hp_app::widgets::toggle(ui, &mut self.node_previews, Some(t!("shader.tb.node_previews")));
        });
    }

    fn side_panel(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.preview_section(ui);
        ui.add_space(6.0);
        hp_app::widgets::section(ui, t!("shader.sec.node"));
        self.node_section(ui, state);
        ui.add_space(6.0);
        hp_app::widgets::section(ui, t!("shader.sec.export"));
        self.export_section(ui, state);
        if !self.diagnostics.is_empty() {
            ui.add_space(6.0);
            hp_app::widgets::section(ui, t!("shader.sec.problems"));
            self.problems_section(ui);
        }
    }

    fn preview_section(&mut self, ui: &mut Ui) {
        let title = match &self.probe {
            Some(p) => self
                .doc
                .graph
                .def_of(p.node)
                .map(|d| format!("{} ({})", labels::node_title(d.id), p.node))
                .unwrap_or_default(),
            None => t!("shader.preview.output").to_owned(),
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(t!("shader.sec.preview")).strong());
            ui.weak(title);
            if self.probe.is_some() && ui.small_button(t!("shader.preview.reset")).clicked() {
                self.probe = None;
            }
        });
        ui.horizontal_wrapped(|ui| {
            egui::ComboBox::from_id_salt("hp_shader_channel")
                .selected_text(tr_channel(self.channel))
                .width(96.0)
                .show_ui(ui, |ui| {
                    for c in Channel::ALL {
                        ui.selectable_value(&mut self.channel, c, tr_channel(c));
                    }
                });
            egui::ComboBox::from_id_salt("hp_shader_tile")
                .selected_text(format!("{0}x{0}", self.tile))
                .width(64.0)
                .show_ui(ui, |ui| {
                    for n in 1..=3u32 {
                        ui.selectable_value(&mut self.tile, n, format!("{n}x{n}"));
                    }
                });
            egui::ComboBox::from_id_salt("hp_shader_res")
                .selected_text(format!("{} px", self.res))
                .width(82.0)
                .show_ui(ui, |ui| {
                    for r in PREVIEW_RES {
                        ui.selectable_value(&mut self.res, r, format!("{r} px"));
                    }
                });
        });
        let side = ui.available_width().min(330.0);
        let (aspect_w, aspect_h) =
            self.preview.result.as_ref().map(|r| (r.width.max(1) as f32, r.height.max(1) as f32)).unwrap_or((1.0, 1.0));
        let fit = (side / aspect_w).min(side / aspect_h);
        let (rect, _) = ui.allocate_exact_size(vec2(aspect_w * fit, aspect_h * fit), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, theme::palette().bg0);
        let cell = 10.0;
        let (a, b) = (Color32::from_gray(54), Color32::from_gray(74));
        painter.rect_filled(rect, 0.0, a);
        let mut y = 0;
        let mut yy = rect.top();
        while yy < rect.bottom() {
            let mut x = y % 2;
            let mut xx = rect.left() + cell * x as f32;
            while xx < rect.right() {
                let r = Rect::from_min_size(pos2(xx, yy), vec2(cell, cell)).intersect(rect);
                painter.rect_filled(r, 0.0, b);
                x += 2;
                xx = rect.left() + cell * x as f32;
            }
            yy += cell;
            y += 1;
        }
        if let Some(tex) = &self.preview.image {
            let n = self.tile.max(1);
            let step = rect.size() / n as f32;
            for iy in 0..n {
                for ix in 0..n {
                    let r = Rect::from_min_size(rect.min + vec2(step.x * ix as f32, step.y * iy as f32), step);
                    painter.image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
            }
        } else {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                t!("shader.preview.none"),
                egui::FontId::proportional(13.0),
                theme::palette().text_weak,
            );
        }
        painter.rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(1.0, theme::palette().border_strong),
            egui::StrokeKind::Inside,
        );
        let status = match &self.preview.result {
            Some(r) => {
                let busy =
                    if self.preview.busy() { format!("  {}", t!("shader.preview.updating")) } else { String::new() };
                format!("{}x{}  {} ms{}", r.width, r.height, self.preview.elapsed.as_millis(), busy)
            }
            None => t!("shader.preview.updating").to_owned(),
        };
        ui.weak(status);
    }

    fn node_section(&mut self, ui: &mut Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        let selected = self.canvas.selected.clone();
        match selected.as_slice() {
            [] => {
                ui.weak(t!("shader.sel.none"));
            }
            [id] => {
                let id = *id;
                if let Some(def) = self.doc.graph.def_of(id) {
                    inspector::node_header(ui, def);
                    let probe_here = self.probe.as_ref().is_some_and(|p| p.node == id);
                    if !def.is_output() && !def.outputs.is_empty() {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui.add_enabled(!probe_here, egui::Button::new(t!("shader.menu.preview"))).clicked() {
                                self.probe = Some(Probe { node: id, output: def.outputs[0].name.to_owned() });
                            }
                            if def.outputs.len() > 1 {
                                for o in def.outputs {
                                    let on = self.probe.as_ref().is_some_and(|p| p.node == id && p.output == o.name);
                                    if ui.add(egui::Button::selectable(on, labels::pin(o.name))).clicked() {
                                        self.probe = Some(Probe { node: id, output: o.name.to_owned() });
                                    }
                                }
                            }
                        });
                    }
                    ui.add_space(4.0);
                }
                if let Some(ask) = inspector::node_inspector(ui, &mut self.doc.graph, id) {
                    if ask.browse {
                        self.ask_image(&ctx, ask.node);
                    }
                    if ask.current {
                        let name = state.current_material.clone();
                        if !name.is_empty() {
                            let tex = state
                                .assets
                                .material_info(&name)
                                .and_then(|i| i.base_texture.clone())
                                .filter(|t| !t.trim().is_empty())
                                .unwrap_or(name.clone());
                            let _ = self.doc.graph.set_param(ask.node, "source", serde_json::json!(tex));
                            if self.export.name.is_empty() {
                                self.export.name = format!("{}_edit", file_leaf(&name));
                            }
                        }
                    }
                }
            }
            many => {
                ui.weak(tf!("shader.sel.many", many.len()));
            }
        }
    }

    fn export_section(&mut self, ui: &mut Ui, state: &mut AppState) {
        let dirs_len = state.settings.game.custom_texture_dirs.len();
        if self.export.name.is_empty() {
            self.export.name = self.default_name();
        }
        egui::Grid::new("hp_shader_export").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(t!("tex.import.dest"));
            if dirs_len == 0 {
                ui.colored_label(theme::warning(), t!("tex.import.no_folder"));
            } else {
                self.export.dest = self.export.dest.min(dirs_len - 1);
                let dirs = &state.settings.game.custom_texture_dirs;
                let current =
                    dirs.get(self.export.dest).map(|d| crate::util::folder_label(&d.path)).unwrap_or_default();
                egui::ComboBox::from_id_salt("hp_shader_dest").selected_text(current).width(190.0).show_ui(ui, |ui| {
                    for (i, d) in dirs.iter().enumerate() {
                        ui.selectable_value(&mut self.export.dest, i, crate::util::folder_label(&d.path))
                            .on_hover_text(d.path.display().to_string());
                    }
                });
            }
            ui.end_row();
            ui.label(t!("shader.export.name"));
            ui.add(egui::TextEdit::singleline(&mut self.export.name).desired_width(190.0));
            ui.end_row();
            ui.label(t!("shader.export.size"));
            let cur = self.doc.graph.settings.width;
            let label = if cur == 0 { t!("shader.export.auto").to_owned() } else { format!("{cur} px") };
            egui::ComboBox::from_id_salt("hp_shader_size").selected_text(label).width(190.0).show_ui(ui, |ui| {
                for s in BAKE_SIZES {
                    let text = if s == 0 { t!("shader.export.auto").to_owned() } else { format!("{s} px") };
                    if ui.selectable_label(cur == s, text).clicked() {
                        self.doc.graph.settings.width = s;
                        self.doc.graph.settings.height = 0;
                    }
                }
            });
            ui.end_row();
            ui.label(t!("tex.import.surfaceprop"));
            let shown = if self.export.surface_prop.is_empty() {
                t!("common.none").to_owned()
            } else {
                self.export.surface_prop.clone()
            };
            egui::ComboBox::from_id_salt("hp_shader_sp").selected_text(shown).width(190.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.export.surface_prop, String::new(), t!("common.none"));
                for sp in crate::util::SURFACE_PROPS {
                    ui.selectable_value(&mut self.export.surface_prop, (*sp).to_owned(), *sp);
                }
            });
            ui.end_row();
        });
        ui.checkbox(&mut self.export.overwrite, t!("tex.import.overwrite"));
        ui.checkbox(&mut self.export.save_graph, t!("shader.export.save_graph"))
            .on_hover_text(t!("shader.export.save_graph_tip"));
        let stem = hp_shader::sanitize_stem(&self.export.name);
        let outputs = self.doc.graph.output_ids();
        if !stem.is_empty() {
            ui.add_space(2.0);
            for id in &outputs {
                let suffix = self.doc.graph.output_suffix(*id);
                ui.weak(format!("{stem}{suffix}.vtf"));
            }
        }
        let blocking = self.diagnostics.iter().any(|d| d.severity == hp_shader::Severity::Error);
        let can = dirs_len > 0 && !stem.is_empty() && !outputs.is_empty() && !blocking && self.bake.is_none();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let label = if self.bake.is_some() { t!("shader.export.writing") } else { t!("shader.export.write") };
            ui.add_enabled_ui(can, |ui| {
                if hp_app::widgets::primary_button(ui, label).clicked() {
                    self.start_bake(state);
                }
            });
            if self.bake.is_some() {
                ui.spinner();
            }
        });
        if let Some(e) = &self.export.error {
            ui.colored_label(theme::error(), e);
        }
        for l in &self.export.lines {
            ui.label(egui::RichText::new(l).small().color(theme::success()));
        }
    }

    fn problems_section(&mut self, ui: &mut Ui) {
        let mut select: Option<u32> = None;
        for d in &self.diagnostics {
            let color = match d.severity {
                hp_shader::Severity::Error => theme::error(),
                hp_shader::Severity::Warning => theme::warning(),
                hp_shader::Severity::Info => theme::info(),
            };
            let node = d.node.and_then(|n| self.doc.graph.def_of(n).map(|def| (n, labels::node_title(def.id))));
            let text = match node {
                Some((n, title)) => format!("{title} ({n}): {}", labels::diagnostic(d)),
                None => labels::diagnostic(d),
            };
            let r =
                ui.add(egui::Label::new(egui::RichText::new(text).small().color(color)).sense(Sense::click()).wrap());
            if r.clicked() {
                select = d.node;
            }
        }
        if let Some(id) = select {
            self.canvas.selected = vec![id];
        }
    }
}

fn tr_channel(c: Channel) -> &'static str {
    hp_app::i18n::tr(&format!("shader.channel.{}", c.key()))
}
