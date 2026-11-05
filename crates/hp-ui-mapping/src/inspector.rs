use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use egui::{RichText, Ui};
use hp_app::theme;
use hp_app::{AppState, Severity, t, tf};

use crate::dialogs::MapDialog;
use crate::util::{escape_pressed, icon_title};

pub(crate) fn dialogs() -> Vec<Box<dyn MapDialog>> {
    vec![Box::new(Inspector::default())]
}

const MAX_PREVIEW: usize = 32 << 20;
const MAX_TEXT: usize = 512 << 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Bsp,
    Gma,
    Vpk,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub name: String,
    pub size: u64,
    pub compressed: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Loaded {
    pub path: PathBuf,
    pub kind: Kind,
    pub bytes: u64,
    pub entries: Vec<Entry>,
    pub info: Vec<(String, String)>,
    pub entities: Option<String>,
    pub names: HashSet<String>,
}

impl Loaded {
    fn new(
        path: &Path,
        kind: Kind,
        bytes: u64,
        entries: Vec<Entry>,
        info: Vec<(String, String)>,
        entities: Option<String>,
    ) -> Loaded {
        let names = entries.iter().map(|e| e.name.clone()).collect();
        Loaded { path: path.to_path_buf(), kind, bytes, entries, info, entities, names }
    }
}

pub(crate) fn load(path: &Path) -> Result<Loaded, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let bytes = std::fs::metadata(path).map(|m| m.len()).map_err(|e| e.to_string())?;
    match ext.as_str() {
        "bsp" => {
            let bsp = hp_bsp::Bsp::open(path).map_err(|e| e.to_string())?;
            let pak = bsp.pakfile().map_err(|e| e.to_string())?;
            let entries = pak
                .entries()
                .iter()
                .map(|e| Entry { name: e.key.clone(), size: u64::from(e.size), compressed: e.is_compressed() })
                .collect();
            let ents = bsp.entities().unwrap_or_default();
            let info = vec![
                (t!("inspector.version").to_owned(), bsp.version().to_string()),
                (t!("inspector.revision").to_owned(), bsp.map_revision().to_string()),
                (t!("inspector.entities").to_owned(), ents.len().to_string()),
                (t!("inspector.pakfile").to_owned(), crate::release::fmt_size(pak.byte_len() as u64)),
                (
                    t!("inspector.compressed_lumps").to_owned(),
                    (0..hp_bsp::LUMP_COUNT).filter(|&i| bsp.is_compressed(i)).count().to_string(),
                ),
            ];
            let entities = bsp.entity_text().ok();
            Ok(Loaded::new(path, Kind::Bsp, bytes, entries, info, entities))
        }
        "gma" | "bin" => {
            let gma = hp_gma::GmaArchive::open(path).map_err(|e| e.to_string())?;
            let entries = gma
                .iter()
                .map(|(name, e)| Entry { name: name.to_owned(), size: e.size(), compressed: false })
                .collect();
            let info = vec![
                (t!("inspector.addon_title").to_owned(), gma.name().to_owned()),
                (t!("inspector.addon_type").to_owned(), gma.addon_type().unwrap_or("").to_owned()),
                (t!("inspector.addon_tags").to_owned(), gma.tags().join(", ")),
                (t!("inspector.version").to_owned(), gma.format_version().to_string()),
            ];
            Ok(Loaded::new(path, Kind::Gma, bytes, entries, info, None))
        }
        "vpk" => {
            let vpk = hp_vpk::VpkArchive::open(path).map_err(|e| e.to_string())?;
            let entries = vpk
                .iter()
                .map(|(name, e)| Entry { name: name.to_owned(), size: e.size(), compressed: false })
                .collect();
            let info = vec![(t!("inspector.version").to_owned(), vpk.version().to_string())];
            Ok(Loaded::new(path, Kind::Vpk, bytes, entries, info, None))
        }
        _ => Err(t!("inspector.unsupported").to_owned()),
    }
}

pub(crate) fn read_entry(path: &Path, kind: Kind, name: &str) -> Result<Vec<u8>, String> {
    match kind {
        Kind::Bsp => {
            let bsp = hp_bsp::Bsp::open(path).map_err(|e| e.to_string())?;
            let pak = bsp.pakfile().map_err(|e| e.to_string())?;
            let data = pak.read_name(name).map_err(|e| e.to_string())?;
            data.map(|d| d.into_owned()).ok_or_else(|| t!("inspector.not_found").to_owned())
        }
        Kind::Gma => hp_gma::GmaArchive::open(path)
            .and_then(|g| g.read_checked(name))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| t!("inspector.not_found").to_owned()),
        Kind::Vpk => hp_vpk::VpkArchive::open(path)
            .and_then(|v| v.read_checked(name))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| t!("inspector.not_found").to_owned()),
    }
}

pub(crate) enum Preview {
    Text(String),
    Image { width: u32, height: u32, rgba: Vec<u8>, note: String },
    Info(String),
}

const TEXT_EXTS: &[&str] =
    &["vmt", "txt", "lua", "res", "cfg", "json", "vmf", "rad", "properties", "fgd", "vcd", "lst", "xml", "csv"];

pub(crate) fn preview(name: &str, data: &[u8]) -> Preview {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if TEXT_EXTS.contains(&ext.as_str()) {
        let cut = &data[..data.len().min(MAX_TEXT)];
        return Preview::Text(String::from_utf8_lossy(cut).into_owned());
    }
    match ext.as_str() {
        "vtf" => match hp_vtf::read_info(data) {
            Ok(info) => {
                let note = tf!(
                    "inspector.vtf_info",
                    info.width,
                    info.height,
                    format!("{:?}", info.format),
                    info.mip_count,
                    info.frames
                );
                let opts = hp_vtf::DecodeOptions {
                    mip: hp_vtf::MipSelect::MaxDimension(256),
                    ..hp_vtf::DecodeOptions::default()
                };
                match hp_vtf::decode_rgba8(data, &opts) {
                    Ok(img) => Preview::Image { width: img.width, height: img.height, rgba: img.pixels, note },
                    Err(_) => Preview::Info(note),
                }
            }
            Err(e) => Preview::Info(e.to_string()),
        },
        "png" | "jpg" | "jpeg" | "tga" | "bmp" => match crate::release::decode_image(data) {
            Ok(img) => {
                let note = format!("{} x {}", img.width(), img.height());
                let small = image::imageops::thumbnail(&img, img.width().min(256), img.height().min(256));
                Preview::Image { width: small.width(), height: small.height(), rgba: small.into_raw(), note }
            }
            Err(e) => Preview::Info(e),
        },
        "mdl" => match hp_mdl::read_header(data) {
            Ok(h) => Preview::Info(tf!(
                "inspector.mdl_info",
                h.name,
                h.version,
                h.num_textures,
                h.num_skin_families,
                h.num_bones
            )),
            Err(e) => Preview::Info(e.to_string()),
        },
        "wav" => Preview::Info(wav_info(data).unwrap_or_else(|| t!("inspector.no_preview").to_owned())),
        "pcf" => match hp_fs::deps::parse_pcf(data) {
            Some(p) => {
                let names: Vec<&str> = p.systems.iter().map(|(n, _)| n.as_str()).take(200).collect();
                Preview::Text(tf!("inspector.pcf_info", p.systems.len()) + "\n\n" + &names.join("\n"))
            }
            None => Preview::Info(t!("inspector.no_preview").to_owned()),
        },
        _ => Preview::Info(t!("inspector.no_preview").to_owned()),
    }
}

fn wav_info(data: &[u8]) -> Option<String> {
    if data.get(..4)? != b"RIFF" || data.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut at = 12;
    while at + 8 <= data.len() {
        let id = &data[at..at + 4];
        let len = u32::from_le_bytes(data[at + 4..at + 8].try_into().ok()?) as usize;
        if id == b"fmt " && at + 24 <= data.len() {
            let channels = u16::from_le_bytes([data[at + 10], data[at + 11]]);
            let rate = u32::from_le_bytes(data[at + 12..at + 16].try_into().ok()?);
            let bits = u16::from_le_bytes([data[at + 22], data[at + 23]]);
            return Some(tf!("inspector.wav_info", channels, rate, bits));
        }
        at = at.checked_add(8 + len + (len & 1))?;
    }
    None
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Edits {
    pub add: BTreeMap<String, PathBuf>,
    pub remove: HashSet<String>,
    pub entities: Option<String>,
}

impl Edits {
    fn is_empty(&self) -> bool {
        self.add.is_empty() && self.remove.is_empty() && self.entities.is_none()
    }
}

pub(crate) fn save_bsp(path: &Path, edits: &Edits) -> Result<(), String> {
    let bsp = hp_bsp::Bsp::read(path).map_err(|e| e.to_string())?;
    let pak = bsp.pakfile().map_err(|e| e.to_string())?;
    let compressed = pak.any_compressed();
    let mut writer = pak.to_writer().map_err(|e| e.to_string())?;
    drop(pak);
    for name in &edits.remove {
        writer.remove(name);
    }
    for (name, file) in &edits.add {
        writer.add_file(name, file.clone()).map_err(|e| e.to_string())?;
    }
    let compression = if compressed { hp_bsp::PakCompression::Lzma } else { hp_bsp::PakCompression::Store };
    let zip = writer.finish(hp_bsp::PakOptions { compression, level: 6 }).map_err(|e| e.to_string())?;
    let mut edit = bsp.edit();
    edit.set_pakfile(zip).map_err(|e| e.to_string())?;
    if let Some(text) = &edits.entities {
        let ents = hp_bsp::parse_entities(text);
        if ents.first().is_none_or(|e| !e.classname().eq_ignore_ascii_case("worldspawn")) {
            return Err(t!("inspector.entities_invalid").to_owned());
        }
        edit.set_entities(&ents).map_err(|e| e.to_string())?;
    }
    edit.save(path).map_err(|e| e.to_string())
}

pub(crate) fn extract(path: &Path, kind: Kind, names: &[String], dest: &Path) -> Result<usize, String> {
    let mut n = 0;
    for name in names {
        let Some(target) = crate::release::safe_join(dest, name) else { continue };
        let data = read_entry(path, kind, name)?;
        crate::release::write_atomic(&target, &data).map_err(|e| tf!("inspector.write_failed", target.display(), e))?;
        n += 1;
    }
    Ok(n)
}

enum Reply {
    Loaded(Box<Result<Loaded, String>>),
    Preview(String, Preview),
    Extracted(Result<usize, String>, PathBuf),
    Saved(Result<(), String>),
    Picked(PickPurpose, Vec<PathBuf>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PickPurpose {
    Open,
    Add,
    Replace,
    ExtractTo,
}

#[derive(Default)]
pub(crate) struct Inspector {
    loaded: Option<std::sync::Arc<Loaded>>,
    error: Option<String>,
    busy: usize,
    rx: Option<Receiver<Reply>>,
    tx: Option<mpsc::Sender<Reply>>,
    filter: String,
    filtered: Vec<usize>,
    filter_key: Option<(String, usize)>,
    selected: Option<String>,
    preview: Option<(String, Preview)>,
    preview_tex: Option<egui::TextureHandle>,
    edits: Edits,
    entity_text: String,
    show_entities: bool,
    confirm_save: bool,
    used: Option<(usize, std::sync::Arc<HashSet<String>>)>,
}

impl MapDialog for Inspector {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if let Some(path) = state.runtime.content.inspector_request.take() {
            state.runtime.content.inspector_open = true;
            self.open(ctx, path);
        }
        self.poll(ctx, state);
        if !state.runtime.content.inspector_open {
            return;
        }
        let mut open = true;
        let response = hp_app::chrome::Window::new(icon_title("🗄", t!("inspector.title")))
            .id(egui::Id::unique("hp_inspector_window"))
            .default_pos(crate::util::centered_pos(ctx, egui::vec2(900.0, 620.0)))
            .default_size([900.0, 620.0])
            .min_size([560.0, 360.0])
            .open(&mut open)
            .resizable(true)
            .show(ctx, |ui| crate::util::dialog_body(ui, |ui| self.ui(ui, state)));
        if let Some(r) = &response
            && escape_pressed(ctx, &r.response)
            && !self.confirm_save
        {
            open = false;
        }
        self.confirm_modal(ctx);
        if !open {
            state.runtime.content.inspector_open = false;
        }
    }
}

impl Inspector {
    fn channel(&mut self) -> mpsc::Sender<Reply> {
        if self.tx.is_none() {
            let (tx, rx) = mpsc::channel();
            self.tx = Some(tx);
            self.rx = Some(rx);
        }
        self.tx.clone().unwrap_or_else(|| mpsc::channel().0)
    }

    fn spawn(&mut self, ctx: &egui::Context, work: impl FnOnce() -> Reply + Send + 'static) {
        let tx = self.channel();
        let repaint = ctx.clone();
        self.busy += 1;
        let spawned = std::thread::Builder::new().name("hp-inspector".into()).spawn(move || {
            let _ = tx.send(work());
            repaint.request_repaint();
        });
        if spawned.is_err() {
            self.busy -= 1;
        }
    }

    pub(crate) fn open(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.loaded = None;
        self.error = None;
        self.selected = None;
        self.preview = None;
        self.preview_tex = None;
        self.edits = Edits::default();
        self.filter_key = None;
        self.spawn(ctx, move || Reply::Loaded(Box::new(load(&path))));
    }

    fn pick(&mut self, ctx: &egui::Context, purpose: PickPurpose, start: Option<PathBuf>) {
        let filter_name = t!("inspector.archives").to_owned();
        self.spawn(ctx, move || {
            let mut d = rfd::FileDialog::new();
            if let Some(s) = start {
                d = d.set_directory(s);
            }
            let picked = match purpose {
                PickPurpose::Open => {
                    d.add_filter(filter_name, &["bsp", "gma", "vpk"]).pick_file().into_iter().collect()
                }
                PickPurpose::Add => d.pick_files().unwrap_or_default(),
                PickPurpose::Replace => d.pick_file().into_iter().collect(),
                PickPurpose::ExtractTo => d.pick_folder().into_iter().collect(),
            };
            Reply::Picked(purpose, picked)
        });
    }

    fn poll(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let Some(rx) = &self.rx else { return };
        let mut replies = Vec::new();
        while let Ok(r) = rx.try_recv() {
            replies.push(r);
        }
        for r in replies {
            self.busy = self.busy.saturating_sub(1);
            match r {
                Reply::Loaded(result) => match *result {
                    Ok(l) => {
                        self.entity_text = l.entities.clone().unwrap_or_default();
                        self.loaded = Some(std::sync::Arc::new(l));
                        self.error = None;
                    }
                    Err(e) => self.error = Some(e),
                },
                Reply::Preview(name, p) => {
                    self.preview_tex = match &p {
                        Preview::Image { width, height, rgba, .. }
                            if rgba.len() == (*width as usize) * (*height as usize) * 4 =>
                        {
                            let img =
                                egui::ColorImage::from_rgba_unmultiplied([*width as usize, *height as usize], rgba);
                            Some(ctx.load_texture(
                                "hp_inspector_preview",
                                img,
                                egui::TextureOptions::LINEAR.with_mipmap_mode(Some(egui::TextureFilter::Linear)),
                            ))
                        }
                        _ => None,
                    };
                    self.preview = Some((name, p));
                }
                Reply::Extracted(result, dest) => match result {
                    Ok(n) => {
                        state.notify(Severity::Success, tf!("inspector.extracted", n, dest.display()));
                    }
                    Err(e) => state.notify(Severity::Error, e),
                },
                Reply::Saved(result) => match result {
                    Ok(()) => {
                        state.notify(Severity::Success, t!("inspector.saved"));
                        if let Some(l) = &self.loaded {
                            let path = l.path.clone();
                            self.open(ctx, path);
                        }
                    }
                    Err(e) => state.notify(Severity::Error, tf!("inspector.save_failed", e)),
                },
                Reply::Picked(purpose, paths) => self.picked(ctx, purpose, paths),
            }
        }
        if self.busy > 0 {
            ctx.request_repaint_after(Duration::from_millis(200));
        }
    }

    fn picked(&mut self, ctx: &egui::Context, purpose: PickPurpose, paths: Vec<PathBuf>) {
        match purpose {
            PickPurpose::Open => {
                if let Some(p) = paths.into_iter().next() {
                    self.open(ctx, p);
                }
            }
            PickPurpose::Add => {
                let folder = self
                    .selected
                    .as_deref()
                    .and_then(|s| s.rsplit_once('/'))
                    .map(|(d, _)| format!("{d}/"))
                    .unwrap_or_default();
                for p in paths.into_iter().take(10_000) {
                    let file = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                    let name = format!("{folder}{file}");
                    if hp_bsp::normalize_entry_name(&name).is_ok() {
                        self.edits.remove.remove(&name);
                        self.edits.add.insert(name, p);
                    } else {
                        self.error = Some(tf!("inspector.bad_name", name));
                    }
                }
            }
            PickPurpose::Replace => {
                if let (Some(p), Some(name)) = (paths.into_iter().next(), self.selected.clone()) {
                    self.edits.remove.remove(&name);
                    self.edits.add.insert(name, p);
                }
            }
            PickPurpose::ExtractTo => {
                let (Some(dest), Some(l)) = (paths.into_iter().next(), &self.loaded) else { return };
                let names: Vec<String> = match &self.selected {
                    Some(s) => vec![s.clone()],
                    None => self.filtered.iter().filter_map(|&i| l.entries.get(i)).map(|e| e.name.clone()).collect(),
                };
                let (path, kind) = (l.path.clone(), l.kind);
                self.spawn(ctx, move || {
                    let r = extract(&path, kind, &names, &dest);
                    Reply::Extracted(r, dest)
                });
            }
        }
    }

    fn select(&mut self, ctx: &egui::Context, name: String) {
        if self.selected.as_deref() == Some(name.as_str()) {
            return;
        }
        self.selected = Some(name.clone());
        let Some(l) = &self.loaded else { return };
        let size = l.entries.iter().find(|e| e.name == name).map_or(0, |e| e.size);
        if size as usize > MAX_PREVIEW {
            self.preview = Some((name, Preview::Info(t!("inspector.too_large").to_owned())));
            self.preview_tex = None;
            return;
        }
        let (path, kind) = (l.path.clone(), l.kind);
        self.spawn(ctx, move || {
            let p = match read_entry(&path, kind, &name) {
                Ok(data) => preview(&name, &data),
                Err(e) => Preview::Info(e),
            };
            Reply::Preview(name, p)
        });
    }

    fn refilter(&mut self) {
        let Some(l) = &self.loaded else { return };
        let key = (self.filter.clone(), l.entries.len());
        if self.filter_key.as_ref() == Some(&key) {
            return;
        }
        let needle = self.filter.trim().to_ascii_lowercase();
        self.filtered =
            (0..l.entries.len()).filter(|&i| needle.is_empty() || l.entries[i].name.contains(&needle)).collect();
        self.filtered.sort_by(|&a, &b| l.entries[a].name.cmp(&l.entries[b].name));
        self.filter_key = Some(key);
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let ctx = ui.ctx().clone();
        ui.horizontal(|ui| {
            if ui.button(t!("inspector.open")).clicked() {
                let start = state.doc.path().and_then(|p| p.parent()).map(Path::to_path_buf);
                self.pick(&ctx, PickPurpose::Open, start);
            }
            if self.busy > 0 {
                ui.spinner();
            }
            if let Some(l) = &self.loaded {
                ui.label(RichText::new(l.path.display().to_string()).monospace());
                ui.label(crate::release::fmt_size(l.bytes));
            }
        });
        if let Some(e) = &self.error {
            ui.colored_label(theme::error(), e);
        }
        let Some(loaded) = self.loaded.clone() else {
            ui.label(RichText::new(t!("inspector.empty")).weak());
            return;
        };
        ui.horizontal_wrapped(|ui| {
            for (k, v) in &loaded.info {
                ui.label(RichText::new(format!("{k} :")).weak());
                ui.label(v);
                ui.separator();
            }
        });
        let is_bsp = loaded.kind == Kind::Bsp;
        ui.horizontal(|ui| {
            if crate::release::filter_box(ui, &mut self.filter, t!("inspector.filter"), 260.0) {
                self.filter_key = None;
            }
            if ui.button(t!("inspector.extract")).on_hover_text(t!("inspector.extract_tip")).clicked() {
                self.pick(&ctx, PickPurpose::ExtractTo, None);
            }
            if is_bsp {
                if ui.button(t!("inspector.add")).clicked() {
                    self.pick(&ctx, PickPurpose::Add, None);
                }
                if ui.add_enabled(self.selected.is_some(), egui::Button::new(t!("inspector.replace"))).clicked() {
                    self.pick(&ctx, PickPurpose::Replace, None);
                }
                if ui.add_enabled(self.selected.is_some(), egui::Button::new(t!("inspector.remove"))).clicked()
                    && let Some(s) = self.selected.clone()
                {
                    self.edits.add.remove(&s);
                    self.edits.remove.insert(s);
                }
                ui.toggle_value(&mut self.show_entities, t!("inspector.entity_lump"));
                if ui
                    .add_enabled(!self.edits.is_empty() && self.busy == 0, egui::Button::new(t!("inspector.save")))
                    .clicked()
                {
                    self.confirm_save = true;
                }
                if !self.edits.is_empty() {
                    ui.colored_label(
                        theme::warning(),
                        tf!("inspector.pending", self.edits.add.len(), self.edits.remove.len()),
                    );
                    if ui.small_button(t!("inspector.discard")).clicked() {
                        self.edits = Edits::default();
                        self.entity_text = loaded.entities.clone().unwrap_or_default();
                    }
                }
            }
        });
        ui.separator();
        if self.show_entities && is_bsp {
            let r = egui::ScrollArea::vertical().id_salt("hp_inspector_entities").auto_shrink([false, false]).show(
                ui,
                |ui| {
                    ui.add(egui::TextEdit::multiline(&mut self.entity_text).code_editor().desired_width(f32::INFINITY))
                },
            );
            if r.inner.changed() {
                self.edits.entities = Some(self.entity_text.clone());
            }
            return;
        }
        self.refilter();
        let used: Option<std::sync::Arc<HashSet<String>>> =
            match state.runtime.content.graph.as_ref().filter(|_| is_bsp) {
                Some(g) => {
                    let key = std::sync::Arc::as_ptr(g) as usize;
                    if self.used.as_ref().map(|(k, _)| *k) != Some(key) {
                        let set: HashSet<String> = g.nodes.iter().map(|n| n.vpath.clone()).collect();
                        self.used = Some((key, std::sync::Arc::new(set)));
                    }
                    self.used.as_ref().map(|(_, s)| s.clone())
                }
                None => None,
            };
        let mut clicked: Option<String> = None;
        ui.columns(2, |cols| {
            let ui = &mut cols[0];
            let row_h = ui.text_style_height(&egui::TextStyle::Body) + 4.0;
            egui::ScrollArea::vertical().id_salt("hp_inspector_list").auto_shrink([false, false]).show_rows(
                ui,
                row_h,
                self.filtered.len(),
                |ui, range| {
                    for &i in &self.filtered[range] {
                        let Some(e) = loaded.entries.get(i) else { continue };
                        let removed = self.edits.remove.contains(&e.name);
                        let replaced = self.edits.add.contains_key(&e.name);
                        let mut text = RichText::new(&e.name).monospace();
                        if removed {
                            text = text.strikethrough().color(theme::error());
                        } else if replaced {
                            text = text.color(theme::warning());
                        } else if let Some(u) = &used {
                            text = text.color(if u.contains(&e.name) { theme::success() } else { theme::info() });
                        }
                        ui.horizontal(|ui| {
                            let sel = self.selected.as_deref() == Some(e.name.as_str());
                            if ui.selectable_label(sel, text).clicked() {
                                clicked = Some(e.name.clone());
                            }
                            ui.label(RichText::new(crate::release::fmt_size(e.size)).weak());
                            if e.compressed {
                                ui.label(RichText::new("LZMA").weak());
                            }
                        });
                    }
                    for name in self.edits.add.keys().filter(|n| !loaded.names.contains(*n)) {
                        ui.label(RichText::new(format!("+ {name}")).monospace().color(theme::success()));
                    }
                },
            );
            let ui = &mut cols[1];
            match &self.preview {
                Some((name, p)) => {
                    ui.label(RichText::new(name).strong());
                    match p {
                        Preview::Text(s) => {
                            egui::ScrollArea::both().id_salt("hp_inspector_text").auto_shrink([false, false]).show(
                                ui,
                                |ui| {
                                    ui.label(RichText::new(s.as_str()).monospace());
                                },
                            );
                        }
                        Preview::Image { note, .. } => {
                            if let Some(tex) = &self.preview_tex {
                                let size = tex.size_vec2();
                                let scale = (256.0 / size.x.max(size.y)).min(2.0);
                                ui.add(egui::Image::new((tex.id(), size * scale)));
                            }
                            ui.label(note);
                        }
                        Preview::Info(s) => {
                            ui.label(s);
                        }
                    }
                }
                None => {
                    ui.label(RichText::new(t!("inspector.select_hint")).weak());
                }
            }
            if used.is_some() {
                ui.separator();
                ui.label(RichText::new(t!("inspector.used_hint")).weak());
            }
        });
        if let Some(name) = clicked {
            self.select(&ctx, name);
        }
    }

    fn confirm_modal(&mut self, ctx: &egui::Context) {
        if !self.confirm_save {
            return;
        }
        let modal = egui::Modal::new(egui::Id::unique("hp_inspector_confirm")).show(ctx, |ui| {
            ui.set_max_width(420.0);
            ui.heading(t!("inspector.confirm_title"));
            ui.label(t!("inspector.confirm_text"));
            ui.add_space(8.0);
            let mut answer = None;
            ui.horizontal(|ui| {
                if ui.button(t!("inspector.save")).clicked() {
                    answer = Some(true);
                }
                if ui.button(t!("inspector.cancel")).clicked() {
                    answer = Some(false);
                }
            });
            answer
        });
        let answer = modal.inner.or_else(|| modal.should_close().then_some(false));
        if let Some(go) = answer {
            self.confirm_save = false;
            if go && let Some(l) = &self.loaded {
                let path = l.path.clone();
                let edits = self.edits.clone();
                self.edits = Edits::default();
                self.spawn(ctx, move || Reply::Saved(save_bsp(&path, &edits)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bsp(dir: &Path) -> PathBuf {
        let mut w = hp_bsp::PakWriter::new();
        w.add("materials/test/a.vmt", b"\"LightmappedGeneric\" { \"$basetexture\" \"test/a\" }".to_vec()).unwrap();
        w.add("sound/test/a.wav", wav()).unwrap();
        let zip = w.finish(hp_bsp::PakOptions::default()).unwrap();
        let bytes = hp_bsp::synthetic_bsp(
            20,
            &[
                (hp_bsp::lump::ENTITIES, b"{\n\"classname\" \"worldspawn\"\n}\n\0".to_vec()),
                (hp_bsp::lump::PAKFILE, zip),
            ],
        );
        let p = dir.join("gm_inspect.bsp");
        std::fs::write(&p, bytes).unwrap();
        p
    }

    fn wav() -> Vec<u8> {
        let mut v = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        v.extend_from_slice(&16u32.to_le_bytes());
        v.extend_from_slice(&1u16.to_le_bytes());
        v.extend_from_slice(&2u16.to_le_bytes());
        v.extend_from_slice(&44_100u32.to_le_bytes());
        v.extend_from_slice(&[0; 6]);
        v.extend_from_slice(&16u16.to_le_bytes());
        v
    }

    #[test]
    fn load_preview_edit_extract() {
        hp_app::register_translations();
        crate::register_translations();
        let dir = crate::test_scratch("inspector");
        let path = sample_bsp(&dir);
        let l = load(&path).unwrap();
        assert_eq!(l.kind, Kind::Bsp);
        assert_eq!(l.entries.len(), 2);
        assert!(l.entities.as_deref().unwrap().contains("worldspawn"));
        let data = read_entry(&path, Kind::Bsp, "materials/test/a.vmt").unwrap();
        assert!(matches!(preview("materials/test/a.vmt", &data), Preview::Text(t) if t.contains("basetexture")));
        assert!(matches!(preview("sound/test/a.wav", &wav()), Preview::Info(t) if t.contains("44100")));
        assert!(matches!(preview("x.bin", b"zz"), Preview::Info(_)));
        let extra = dir.join("extra.txt");
        std::fs::write(&extra, b"hello").unwrap();
        let mut edits = Edits::default();
        edits.add.insert("maps/gm_inspect_particles.txt".into(), extra);
        edits.remove.insert("sound/test/a.wav".into());
        edits.entities = Some("{\n\"classname\" \"worldspawn\"\n}\n{\n\"classname\" \"info_player_start\"\n}\n".into());
        save_bsp(&path, &edits).unwrap();
        let l = load(&path).unwrap();
        let names: Vec<&str> = l.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["maps/gm_inspect_particles.txt", "materials/test/a.vmt"]);
        assert!(l.entities.unwrap().contains("info_player_start"));
        let bad = Edits { entities: Some("{ \"classname\" \"light\" }".into()), ..Edits::default() };
        assert!(save_bsp(&path, &bad).is_err());
        assert_eq!(load(&path).unwrap().entries.len(), 2);
        let out = dir.join("out");
        let n = extract(&path, Kind::Bsp, &["materials/test/a.vmt".into(), "../evil.txt".into()], &out).unwrap();
        assert_eq!(n, 1);
        assert!(out.join("materials/test/a.vmt").is_file());
        assert!(!dir.join("evil.txt").exists());
        assert!(load(&dir.join("x.zip")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn window_draws_headless() {
        let mut state = hp_app::testing::test_state();
        crate::register_translations();
        let dir = crate::test_scratch("inspector_ui");
        let path = sample_bsp(&dir);
        let mut win = Inspector { loaded: Some(std::sync::Arc::new(load(&path).unwrap())), ..Inspector::default() };
        state.runtime.content.inspector_open = true;
        hp_app::testing::run_ctx(&mut state, |ctx, state| win.show(ctx, state));
        win.show_entities = true;
        hp_app::testing::run_ctx(&mut state, |ctx, state| win.show(ctx, state));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
