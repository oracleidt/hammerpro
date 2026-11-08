use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use hp_app::runtime::VtfRequest;
use hp_app::{AppState, Severity, t, tf};
use hp_vtf::ops::{Channels, ImageFileFormat, ResizeFilter};
use hp_vtf::{ImageFormat, RgbaImage, VtfDocument};

use crate::util::{DialogEscape, PendingPick};

pub mod external;
pub mod store;
mod tr;
mod ui;

pub use tr::TR;

use external::{ExternalEditor, Session};
use store::{Kind, Located, MaterialPlan, Target};

const POLL_EVERY: Duration = Duration::from_millis(500);
const MAX_SESSIONS: usize = 16;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Origin {
    Texture(String),
    File(PathBuf),
    New,
}

pub(crate) struct Loaded {
    pub doc: VtfDocument,
    pub origin: Origin,
    pub located: Option<Located>,
    pub materials: Vec<String>,
    pub param: Option<String>,
    pub rev: u64,
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PickKind {
    OpenFile,
    Import,
    NewFromImage,
    SaveVtf,
    SaveTexturePath,
    ExportImage,
    ExportAll,
    AlphaMask,
    AddFrame,
}

struct SaveJob {
    rx: Receiver<Result<SavedTexture, String>>,
    target: Target,
    texture: Option<String>,
    to_file: bool,
    serial: u64,
    revision: u64,
}

struct SavedTexture {
    bytes: Vec<u8>,
    outcome: crate::matedit::WriteOutcome,
    material: Result<Option<(PathBuf, String, bool)>, String>,
}

#[derive(Clone, Debug)]
pub(crate) struct SaveForm {
    pub open: bool,
    pub dest: usize,
    pub name: String,
    pub path: String,
    pub create_vmt: bool,
    pub shader: String,
    pub overwrite: bool,
}

impl Default for SaveForm {
    fn default() -> Self {
        Self {
            open: false,
            dest: 0,
            name: String::new(),
            path: String::new(),
            create_vmt: true,
            shader: "LightmappedGeneric".to_owned(),
            overwrite: false,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct OpsForm {
    pub width: u32,
    pub height: u32,
    pub filter: ResizeFilter,
    pub normal: hp_vtf::ops::NormalOptions,
    pub normal_flag: bool,
    pub alpha_mode: usize,
    pub alpha_value: u8,
    pub export_format: ImageFileFormat,
    pub mip_filter: hp_vtf::MipFilter,
    pub new_format: ImageFormat,
}

impl Default for OpsForm {
    fn default() -> Self {
        Self {
            width: 256,
            height: 256,
            filter: ResizeFilter::Color,
            normal: hp_vtf::ops::NormalOptions::default(),
            normal_flag: true,
            alpha_mode: 0,
            alpha_value: 255,
            export_format: ImageFileFormat::Png,
            mip_filter: hp_vtf::MipFilter::FromFlags,
            new_format: ImageFormat::Dxt5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Background {
    Checker,
    Black,
    White,
    Gray,
}

pub(crate) type ViewKey = (u64, usize, usize, u32, u32, [bool; 4]);

type SyncDone = (String, PathBuf, Vec<String>, Result<bool, String>, Option<(String, String)>);

pub(crate) struct ViewState {
    pub frame: usize,
    pub face: usize,
    pub mip: u32,
    pub slice: u32,
    pub channels: Channels,
    pub zoom: f32,
    pub pan: egui::Vec2,
    pub background: Background,
    pub playing: bool,
    pub fps: f32,
    pub tex: Option<(ViewKey, egui::TextureHandle)>,
    pub checker: Option<egui::TextureHandle>,
    pub last_step: f64,
    pub tab: usize,
    pub pixels: Option<RgbaImage>,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            frame: 0,
            face: 0,
            mip: 0,
            slice: 0,
            channels: Channels::default(),
            zoom: 0.0,
            pan: egui::Vec2::ZERO,
            background: Background::Checker,
            playing: false,
            fps: 10.0,
            tex: None,
            checker: None,
            last_step: 0.0,
            tab: 0,
            pixels: None,
        }
    }
}

#[derive(Default)]
pub struct VtfEditor {
    pub(crate) loaded: Option<Loaded>,
    pub(crate) sessions: Vec<Session>,
    pub(crate) load_text: String,
    pick: Option<(PendingPick<Vec<PathBuf>>, PickKind)>,
    pub(crate) status: Option<(Severity, String)>,
    escape: DialogEscape,
    job: Option<SaveJob>,
    pub(crate) save: SaveForm,
    pub(crate) ops: OpsForm,
    pub(crate) view: ViewState,
    last_poll: Option<Instant>,
    was_open: bool,
    material_editors: Vec<(PathBuf, crate::matedit_single::SingleEditor)>,
    material_tab: Option<usize>,
    loaded_serial: u64,
}

pub fn external_editor(state: &AppState) -> Option<ExternalEditor> {
    external::resolve(&state.settings.vtf.editor_path)
}

pub fn external_editor_ui(state: &AppState) -> Option<ExternalEditor> {
    external::resolve_nonblocking(&state.settings.vtf.editor_path)
}

pub fn texture_name_of(path: &Path) -> Option<String> {
    crate::matedit_single::texture_path_of(path)
}

pub fn pixels_to_color_image(img: &RgbaImage) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied([img.width as usize, img.height as usize], &img.pixels)
}

impl VtfEditor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_open(&self) -> bool {
        self.loaded.is_some()
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn current(&self) -> Option<(&VtfDocument, &Origin)> {
        self.loaded.as_ref().map(|l| (&l.doc, &l.origin))
    }

    pub fn set_status(&mut self, severity: Severity, text: impl Into<String>) {
        self.status = Some((severity, text.into()));
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if let Some(req) = state.runtime.vtf.request.take() {
            self.handle_request(ctx, state, req);
        }
        self.poll_sessions(ctx, state);
        self.poll_pick(ctx, state);
        self.poll_job(ctx, state);
        if !state.runtime.vtf.show {
            self.was_open = false;
            return;
        }
        if !self.was_open {
            self.was_open = true;
        }
        let mut open = true;
        self.escape.begin(ctx);
        let title = match &self.loaded {
            Some(l) if !l.label.is_empty() => format!("{} - {}", t!("vtf.title"), l.label),
            _ => t!("vtf.title").to_owned(),
        };
        let response = hp_app::chrome::Window::new(title)
            .id(egui::Id::unique("hp_vtf_editor"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([1120.0, 720.0])
            .show(ctx, |ui| self.ui(ui, state));
        let escape = self.escape.update(ctx, response.as_ref().map(|r| &r.response));
        if !open || escape {
            state.runtime.vtf.show = false;
        }
        self.handle_drops(ctx, state);
    }

    fn handle_drops(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let files: Vec<PathBuf> = ctx.input(|i| {
            i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect()
        });
        let Some(first) = files.into_iter().next() else { return };
        let ext = first.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        if ext == "vtf" {
            self.open_file(state, &first);
        } else if ImageFileFormat::from_extension(&ext).is_some() {
            if self.loaded.is_some() {
                self.import_into_current(state, &first);
            } else {
                self.new_from_image(state, &first);
            }
        }
    }

    fn handle_request(&mut self, ctx: &egui::Context, state: &mut AppState, req: VtfRequest) {
        let VtfRequest { texture, file, external, materials, param } = req;
        if let Some(path) = file {
            let is_vtf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("vtf"));
            if external == Some(true) && is_vtf {
                self.open_external_file(ctx, state, &path);
                return;
            }
            state.runtime.vtf.show = true;
            if is_vtf {
                self.open_file(state, &path);
            } else {
                self.new_from_image(state, &path);
            }
            return;
        }
        if let Some(texture) = texture {
            let use_external = match external {
                Some(v) => v,
                None => !state.settings.vtf.prefer_integrated && external_editor(state).is_some(),
            };
            if use_external {
                self.open_external(ctx, state, &texture, materials, param);
            } else {
                state.runtime.vtf.show = true;
                self.open_texture(state, &texture, materials, param);
            }
            return;
        }
        state.runtime.vtf.show = true;
    }

    pub fn open_texture(&mut self, state: &mut AppState, texture: &str, materials: Vec<String>, param: Option<String>) {
        let Some(fs) = state.assets.fs().cloned() else {
            self.set_status(Severity::Error, t!("vtf.err.no_content"));
            return;
        };
        let loc = store::locate(&fs, texture);
        let name = loc.name.clone();
        let doc = match (&loc.bytes, &loc.image_file) {
            (Some(bytes), _) => VtfDocument::from_bytes(bytes).map_err(|e| e.to_string()),
            (None, Some(file)) => std::fs::read(file)
                .map_err(|e| e.to_string())
                .and_then(|b| {
                    let ext = file.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
                    hp_vtf::load_image_file(&b, &ext).map_err(|e| e.to_string())
                })
                .map(VtfDocument::from_image),
            _ => Err(tf!("vtf.err.not_found", texture)),
        };
        match doc {
            Ok(doc) => {
                self.install(Loaded {
                    doc,
                    origin: Origin::Texture(name.clone()),
                    located: Some(loc),
                    materials,
                    param,
                    rev: 0,
                    label: name.clone(),
                });
                self.load_text = name;
                self.set_status(Severity::Info, t!("vtf.status.loaded"));
            }
            Err(e) => self.set_status(Severity::Error, tf!("vtf.err.open", texture, e)),
        }
    }

    pub fn open_file(&mut self, _state: &mut AppState, path: &Path) {
        let result = std::fs::read(path).map_err(|e| e.to_string()).and_then(|b| {
            if !hp_vtf::is_vtf(&b) {
                return Err(t!("vtf.err.not_vtf").to_owned());
            }
            VtfDocument::from_bytes(&b).map_err(|e| e.to_string())
        });
        match result {
            Ok(doc) => {
                let label = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let texture = crate::matedit_single::texture_path_of(path);
                let origin = Origin::File(path.to_path_buf());
                self.install(Loaded { doc, origin, located: None, materials: Vec::new(), param: None, rev: 0, label });
                if let Some(t) = texture {
                    self.save.name = t;
                }
                self.set_status(Severity::Info, t!("vtf.status.loaded"));
            }
            Err(e) => self.set_status(Severity::Error, tf!("vtf.err.open", path.display(), e)),
        }
    }

    pub fn new_from_image(&mut self, _state: &mut AppState, path: &Path) {
        let result = std::fs::read(path).map_err(|e| e.to_string()).and_then(|b| {
            let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
            hp_vtf::load_image_file(&b, &ext).map_err(|e| e.to_string())
        });
        match result {
            Ok(img) => {
                let label = path.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let mut doc = VtfDocument::from_image(img);
                if doc.width() > hp_vtf::MAX_POW2_SIZE || doc.height() > hp_vtf::MAX_POW2_SIZE {
                    let _ = doc.resize_all(
                        doc.width().min(hp_vtf::MAX_POW2_SIZE),
                        doc.height().min(hp_vtf::MAX_POW2_SIZE),
                        ResizeFilter::Color,
                    );
                }
                self.install(Loaded {
                    doc,
                    origin: Origin::New,
                    located: None,
                    materials: Vec::new(),
                    param: None,
                    rev: 0,
                    label: label.clone(),
                });
                self.save.name = hp_vtf_safe_name(&label);
                self.save.open = true;
                self.set_status(Severity::Info, t!("vtf.status.imported_new"));
            }
            Err(e) => self.set_status(Severity::Error, tf!("vtf.err.open", path.display(), e)),
        }
    }

    fn install(&mut self, loaded: Loaded) {
        self.ops.width = loaded.doc.width();
        self.ops.height = loaded.doc.height();
        self.ops.new_format = loaded.doc.format;
        self.view = ViewState { checker: self.view.checker.take(), ..ViewState::default() };
        self.material_tab = None;
        self.loaded_serial = self.loaded_serial.wrapping_add(1);
        self.save.open = false;
        self.save.path.clear();
        self.loaded = Some(loaded);
    }

    pub fn import_into_current(&mut self, _state: &mut AppState, path: &Path) {
        let read = std::fs::read(path).map_err(|e| e.to_string()).and_then(|b| {
            let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
            hp_vtf::load_image_file(&b, &ext).map_err(|e| e.to_string())
        });
        let (frame, face) = (self.view.frame, self.view.face);
        let filter = self.ops.filter;
        let Some(l) = self.loaded.as_mut() else { return };
        match read.and_then(|img| l.doc.replace_image(frame, face, img, filter).map_err(|e| e.to_string())) {
            Ok(()) => {
                l.rev += 1;
                self.ops.width = l.doc.width();
                self.ops.height = l.doc.height();
                self.set_status(Severity::Success, t!("vtf.status.imported"));
            }
            Err(e) => self.set_status(Severity::Error, e),
        }
    }

    fn start_pick(&mut self, ctx: &egui::Context, kind: PickKind, suggested: Option<String>) {
        if self.pick.is_some() {
            return;
        }
        const IMAGES: [&str; 5] = ["png", "tga", "jpg", "jpeg", "bmp"];
        let pick = match kind {
            PickKind::OpenFile => PendingPick::spawn(ctx, || {
                rfd::FileDialog::new()
                    .add_filter("VTF", &["vtf"])
                    .add_filter("Images", &IMAGES)
                    .pick_file()
                    .map(|f| vec![f])
            }),
            PickKind::Import | PickKind::NewFromImage | PickKind::AlphaMask | PickKind::AddFrame => {
                PendingPick::spawn(ctx, move || {
                    rfd::FileDialog::new().add_filter("Images", &IMAGES).pick_file().map(|f| vec![f])
                })
            }
            PickKind::SaveVtf | PickKind::SaveTexturePath => {
                let name = suggested.unwrap_or_else(|| "texture.vtf".to_owned());
                PendingPick::spawn(ctx, move || {
                    rfd::FileDialog::new().add_filter("VTF", &["vtf"]).set_file_name(name).save_file().map(|f| vec![f])
                })
            }
            PickKind::ExportImage | PickKind::ExportAll => {
                let name = suggested.unwrap_or_else(|| "texture.png".to_owned());
                PendingPick::spawn(ctx, move || {
                    rfd::FileDialog::new()
                        .add_filter("PNG", &["png"])
                        .add_filter("TGA", &["tga"])
                        .add_filter("BMP", &["bmp"])
                        .add_filter("JPEG", &["jpg"])
                        .set_file_name(name)
                        .save_file()
                        .map(|f| vec![f])
                })
            }
        };
        self.pick = Some((pick, kind));
    }

    pub(crate) fn request_pick(&mut self, ctx: &egui::Context, kind: PickKind, suggested: Option<String>) {
        self.start_pick(ctx, kind, suggested);
    }

    fn poll_pick(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let Some((pick, kind)) = &self.pick else { return };
        let kind = *kind;
        let Some(result) = pick.poll() else {
            ctx.request_repaint_after(Duration::from_millis(200));
            return;
        };
        self.pick = None;
        let Some(path) = result.and_then(|v| v.into_iter().next()) else { return };
        match kind {
            PickKind::OpenFile => {
                let is_vtf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("vtf"));
                if is_vtf {
                    self.open_file(state, &path);
                } else {
                    self.new_from_image(state, &path);
                }
            }
            PickKind::Import => self.import_into_current(state, &path),
            PickKind::NewFromImage => self.new_from_image(state, &path),
            PickKind::SaveVtf => self.save_to_file(ctx, state, path),
            PickKind::SaveTexturePath => self.save.path = path.to_string_lossy().into_owned(),
            PickKind::ExportImage => self.export_current(&path),
            PickKind::ExportAll => self.export_all(&path),
            PickKind::AlphaMask => self.alpha_from_file(&path),
            PickKind::AddFrame => self.add_frame_from_file(&path),
        }
    }

    pub(crate) fn read_image(path: &Path) -> Result<RgbaImage, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        hp_vtf::load_image_file(&bytes, &ext).map_err(|e| e.to_string())
    }

    fn alpha_from_file(&mut self, path: &Path) {
        let mask = match Self::read_image(path) {
            Ok(m) => m,
            Err(e) => return self.set_status(Severity::Error, e),
        };
        let Some(l) = self.loaded.as_mut() else { return };
        let r = l.doc.map_images(|img| hp_vtf::ops::set_alpha_from_image(img, &mask));
        self.finish_edit(r, "vtf.status.alpha_set");
    }

    fn add_frame_from_file(&mut self, path: &Path) {
        let img = match Self::read_image(path) {
            Ok(m) => m,
            Err(e) => return self.set_status(Severity::Error, e),
        };
        let filter = self.ops.filter;
        let Some(l) = self.loaded.as_mut() else { return };
        let r = l.doc.add_frame(Some(img), filter);
        self.finish_edit(r, "vtf.status.frame_added");
    }

    pub(crate) fn finish_edit(&mut self, result: Result<(), hp_vtf::VtfError>, done_key: &str) {
        match result {
            Ok(()) => {
                if let Some(l) = self.loaded.as_mut() {
                    l.rev += 1;
                    self.ops.width = l.doc.width();
                    self.ops.height = l.doc.height();
                    let frames = l.doc.frame_count();
                    let faces = l.doc.face_count();
                    let mips = l.doc.mip_count();
                    self.view.frame = self.view.frame.min(frames.saturating_sub(1));
                    self.view.face = self.view.face.min(faces.saturating_sub(1));
                    self.view.mip = self.view.mip.min(mips.saturating_sub(1));
                }
                self.set_status(Severity::Success, t!(done_key));
            }
            Err(e) => self.set_status(Severity::Error, e.to_string()),
        }
    }

    pub(crate) fn current_image(&self) -> Result<RgbaImage, String> {
        let l = self.loaded.as_ref().ok_or_else(|| t!("vtf.err.nothing_open").to_owned())?;
        l.doc.mip_image(self.view.mip, self.view.frame, self.view.face, self.view.slice).map_err(|e| e.to_string())
    }

    fn export_current(&mut self, path: &Path) {
        let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        let format = ImageFileFormat::from_extension(&ext).unwrap_or(self.ops.export_format);
        let target = path.with_extension(format.extension());
        let result = self
            .current_image()
            .and_then(|img| hp_vtf::ops::encode_image_file(&img, format).map_err(|e| e.to_string()))
            .and_then(|bytes| std::fs::write(&target, bytes).map_err(|e| e.to_string()));
        match result {
            Ok(()) => self.set_status(Severity::Success, tf!("vtf.status.exported", target.display())),
            Err(e) => self.set_status(Severity::Error, e),
        }
    }

    fn export_all(&mut self, path: &Path) {
        let Some(l) = self.loaded.as_ref() else { return };
        let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
        let format = ImageFileFormat::from_extension(&ext).unwrap_or(self.ops.export_format);
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "texture".to_owned());
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        let (frames, faces) = (l.doc.frame_count(), l.doc.face_count());
        let mut written = 0usize;
        for f in 0..frames {
            for c in 0..faces {
                let mut name = stem.clone();
                if frames > 1 {
                    name.push_str(&format!("_f{f:03}"));
                }
                if faces > 1 {
                    name.push_str(&format!("_{}", face_suffix(c)));
                }
                let file = dir.join(format!("{name}.{}", format.extension()));
                let result = l
                    .doc
                    .mip_image(self.view.mip, f, c, self.view.slice)
                    .map_err(|e| e.to_string())
                    .and_then(|img| hp_vtf::ops::encode_image_file(&img, format).map_err(|e| e.to_string()))
                    .and_then(|bytes| std::fs::write(&file, bytes).map_err(|e| e.to_string()));
                match result {
                    Ok(()) => written += 1,
                    Err(e) => return self.set_status(Severity::Error, format!("{}: {e}", file.display())),
                }
            }
        }
        self.set_status(Severity::Success, tf!("vtf.status.exported_many", written, dir.display()));
    }

    pub(crate) fn plan_for(&self) -> MaterialPlan {
        let (translucent, alphatest) = match self.loaded.as_ref() {
            Some(l) => {
                let one = l.doc.flags & hp_vtf::flags::ONEBITALPHA != 0;
                let eight = l.doc.flags & hp_vtf::flags::EIGHTBITALPHA != 0;
                (eight && !one, one)
            }
            None => (false, false),
        };
        MaterialPlan { shader: self.save.shader.clone(), surface_prop: None, translucent, alphatest }
    }

    pub(crate) fn start_save(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if self.job.is_some() {
            return;
        }
        let Some(l) = self.loaded.as_ref() else { return };
        let (target, texture, plan) = match &l.origin {
            Origin::Texture(name) => {
                let Some(loc) = l.located.clone() else { return };
                match store::target_for(state, &loc, self.save.dest) {
                    Ok(t) => {
                        let texture = Some(t.name.clone()).filter(|n| !n.is_empty()).or_else(|| Some(name.clone()));
                        (t, texture, None)
                    }
                    Err(e) => return self.set_status(Severity::Error, e),
                }
            }
            Origin::File(path) => {
                let name = crate::matedit_single::texture_path_of(path).unwrap_or_default();
                (Target { path: path.clone(), name: name.clone() }, Some(name).filter(|n| !n.is_empty()), None)
            }
            Origin::New => match store::chosen_target(state, self.save.dest, &self.save.name, &self.save.path) {
                Ok(t) => {
                    if t.path.is_file() && !self.save.overwrite {
                        return self.set_status(Severity::Warning, tf!("vtf.err.exists", t.path.display()));
                    }
                    let plan = self.save.create_vmt.then(|| self.plan_for());
                    (t.clone(), Some(t.name), plan)
                }
                Err(e) => return self.set_status(Severity::Error, e),
            },
        };
        let to_file = matches!(l.origin, Origin::New) && !self.save.path.trim().is_empty();
        self.spawn_save(ctx, target, texture, plan, to_file);
    }

    pub(crate) fn start_save_as_texture(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if self.job.is_some() {
            return;
        }
        match store::chosen_target(state, self.save.dest, &self.save.name, &self.save.path) {
            Ok(t) => {
                if t.path.is_file() && !self.save.overwrite {
                    return self.set_status(Severity::Warning, tf!("vtf.err.exists", t.path.display()));
                }
                let plan = self.save.create_vmt.then(|| self.plan_for());
                let to_file = !self.save.path.trim().is_empty();
                self.spawn_save(ctx, t.clone(), Some(t.name), plan, to_file);
            }
            Err(e) => self.set_status(Severity::Error, e),
        }
    }

    fn save_to_file(&mut self, ctx: &egui::Context, _state: &mut AppState, mut path: PathBuf) {
        if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("vtf")) {
            path.set_extension("vtf");
        }
        let name = crate::matedit_single::texture_path_of(&path).unwrap_or_default();
        let new = self.loaded.as_ref().is_some_and(|l| matches!(l.origin, Origin::New));
        let name =
            if name.is_empty() { path.file_stem().unwrap_or_default().to_string_lossy().into_owned() } else { name };
        let plan = (new && self.save.create_vmt).then(|| self.plan_for());
        self.spawn_save(ctx, Target { path, name }, None, plan, true);
    }

    fn spawn_save(
        &mut self,
        ctx: &egui::Context,
        target: Target,
        texture: Option<String>,
        plan: Option<MaterialPlan>,
        to_file: bool,
    ) {
        if self.job.is_some() {
            return;
        }
        let Some(l) = self.loaded.as_ref() else { return };
        let doc = l.doc.clone();
        let revision = l.rev;
        let serial = self.loaded_serial;
        let worker_target = target.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx2 = ctx.clone();
        let spawned = std::thread::Builder::new().name("hp-vtf-save".into()).spawn(move || {
            let result = (|| {
                let bytes = doc.to_bytes().map_err(|e| e.to_string())?;
                let outcome = store::write_bytes_with_backup(&worker_target.path, &bytes).map_err(|e| e.to_string())?;
                let material = match plan {
                    None => Ok(None),
                    Some(mut plan) => {
                        if let Ok(info) = hp_vtf::read_info(&bytes) {
                            plan.alphatest = info.flags & hp_vtf::flags::ONEBITALPHA != 0;
                            plan.translucent = info.flags & hp_vtf::flags::EIGHTBITALPHA != 0 && !plan.alphatest;
                        }
                        store::create_vmt(&worker_target, &plan, false).and_then(|created| {
                            let path = worker_target.path.with_extension("vmt");
                            std::fs::read_to_string(&path)
                                .map(|text| Some((path, text, created.is_some())))
                                .map_err(|e| e.to_string())
                        })
                    }
                };
                Ok(SavedTexture { bytes, outcome, material })
            })();
            let _ = tx.send(result);
            ctx2.request_repaint();
        });
        match spawned {
            Ok(_) => {
                self.job = Some(SaveJob { rx, target, texture, to_file, serial, revision });
                self.set_status(Severity::Info, t!("vtf.status.saving"));
            }
            Err(e) => self.set_status(Severity::Error, e.to_string()),
        }
    }

    pub(crate) fn is_saving(&self) -> bool {
        self.job.is_some()
    }

    fn poll_job(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let Some(job) = &self.job else { return };
        let result = match job.rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                return;
            }
            Err(TryRecvError::Disconnected) => Err("the save worker stopped".to_owned()),
        };
        let Some(job) = self.job.take() else { return };
        let saved = match result {
            Ok(saved) => saved,
            Err(e) => return self.set_status(Severity::Error, tf!("vtf.err.save", job.target.path.display(), e)),
        };
        let SavedTexture { bytes, outcome: written, material } = saved;
        if let Some(b) = &written.backup {
            state.log(Severity::Info, tf!("tex.me.backup_made", b.display()));
        }
        let mut vmt_note = String::new();
        let mut vmt_created = false;
        let mut material_failed = false;
        match material {
            Ok(Some((p, text, created))) => {
                vmt_created = created;
                vmt_note =
                    if created { tf!("vtf.status.vmt_created", p.display()) } else { format!("VMT: {}", p.display()) };
                self.open_material(p, text);
            }
            Ok(None) => {}
            Err(e) => {
                material_failed = true;
                vmt_note = e;
            }
        }
        let materials = self.loaded.as_ref().map(|l| l.materials.clone()).unwrap_or_default();
        if let Some(texture) = &job.texture {
            store::refresh(state, &job.target.path, texture, &materials);
        }
        if vmt_created {
            state.assets.remount(&state.settings);
        }
        let mut link: Option<(String, String)> = None;
        if let Some(l) = self.loaded.as_mut()
            && job.serial == self.loaded_serial
            && l.rev == job.revision
        {
            let _ = l.doc.mark_saved(bytes);
            l.rev += 1;
            if job.to_file {
                l.origin = Origin::File(job.target.path.clone());
                l.label = job.target.path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                l.located = None;
            }
            if !job.to_file
                && let Some(texture) = &job.texture
            {
                if let (Some(param), true) = (&l.param, !matches!(&l.origin, Origin::Texture(n) if n == texture)) {
                    link = Some((param.clone(), texture.clone()));
                }
                if matches!(l.origin, Origin::New | Origin::Texture(_)) {
                    l.origin = Origin::Texture(texture.clone());
                    l.label = texture.clone();
                    if let Some(loc) = l.located.as_mut() {
                        loc.name = texture.clone();
                        loc.kind = Kind::Disk;
                        loc.disk = Some(job.target.path.clone());
                    } else {
                        l.located = Some(Located {
                            name: texture.clone(),
                            kind: Kind::Disk,
                            bytes: None,
                            disk: Some(job.target.path.clone()),
                            image_file: None,
                            label: String::new(),
                        });
                    }
                }
            }
        }
        if let Some((param, texture)) = link {
            crate::shared::post_texture_link(ctx, &param, &texture);
        }
        if job.serial == self.loaded_serial {
            self.save.open = false;
        }
        if let Some(texture) = &job.texture
            && !job.to_file
            && job.serial == self.loaded_serial
        {
            self.load_text = texture.clone();
        }
        let text = format!("{} {vmt_note}", tf!("vtf.status.saved", job.target.path.display()));
        let severity = if material_failed { Severity::Warning } else { Severity::Success };
        state.notify(severity, text.trim().to_owned());
        self.set_status(severity, text.trim().to_owned());
    }

    fn open_material(&mut self, path: PathBuf, text: String) {
        let index = self.material_editors.iter().position(|(p, _)| *p == path).unwrap_or_else(|| {
            let mut editor = crate::matedit_single::SingleEditor::default();
            let name = texture_name_of(&path).unwrap_or_else(|| path.display().to_string());
            editor.load(&name, Some(&text));
            editor.file_preview = Some(crate::matedit_preview::FilePreview::new(path.clone()));
            self.material_editors.push((path, editor));
            self.material_editors.len() - 1
        });
        let editor = &mut self.material_editors[index].1;
        if let Some(preview) = &mut editor.file_preview {
            preview.invalidate();
        }
        if !editor.is_dirty() {
            let name = editor.name.clone().unwrap_or_default();
            editor.load(&name, Some(&text));
        }
        self.material_tab = Some(index);
    }

    fn save_material(&mut self, state: &mut AppState) {
        let Some(index) = self.material_tab else { return };
        let Some((path, editor)) = self.material_editors.get_mut(index) else { return };
        if !editor.is_dirty() {
            return;
        }
        if let Err(e) = hp_vmt::parse(&editor.text) {
            editor.error = Some(e.to_string());
            return;
        }
        match crate::matedit::write_with_backup(path, &editor.text) {
            Ok(_) => {
                editor.saved = editor.text.clone();
                if let Some(name) = texture_name_of(path) {
                    state.assets.invalidate_material(&name);
                }
                state.notify(Severity::Success, tf!("tex.me1.saved", path.display()));
            }
            Err(e) => editor.error = Some(e.to_string()),
        }
    }

    pub fn open_external(
        &mut self,
        ctx: &egui::Context,
        state: &mut AppState,
        texture: &str,
        materials: Vec<String>,
        param: Option<String>,
    ) {
        let Some(editor) = external_editor(state) else {
            state.runtime.vtf.show = true;
            self.open_texture(state, texture, materials, param);
            self.set_status(Severity::Warning, t!("vtf.err.no_editor"));
            return;
        };
        let Some(fs) = state.assets.fs().cloned() else {
            state.notify(Severity::Error, t!("vtf.err.no_content"));
            return;
        };
        let loc = store::locate(&fs, texture);
        let (work, dest) = match self.prepare_external(state, &loc) {
            Ok(v) => v,
            Err(e) => return state.notify(Severity::Error, e),
        };
        let mut session = Session::new(&loc.name, work.clone(), dest.path.clone(), materials, &editor.label);
        if !dest.name.is_empty() && dest.name != loc.name {
            session.link = param.map(|p| (p, dest.name.clone()));
            session.texture = dest.name.clone();
        }
        if let Err(e) = external::launch(&editor, &work) {
            return state.notify(Severity::Error, tf!("vtf.err.launch", e));
        }
        self.add_session(ctx, session);
        state.notify(Severity::Info, tf!("vtf.status.external_opened", editor.label, work.display()));
    }

    pub fn open_external_file(&mut self, ctx: &egui::Context, state: &mut AppState, path: &Path) {
        let Some(editor) = external_editor(state) else {
            return state.notify(Severity::Warning, t!("vtf.err.no_editor"));
        };
        let name = crate::matedit_single::texture_path_of(path).unwrap_or_default();
        let session = Session::new(&name, path.to_path_buf(), path.to_path_buf(), Vec::new(), &editor.label);
        if let Err(e) = external::launch(&editor, path) {
            return state.notify(Severity::Error, tf!("vtf.err.launch", e));
        }
        self.add_session(ctx, session);
        state.notify(Severity::Info, tf!("vtf.status.external_opened", editor.label, path.display()));
    }

    fn add_session(&mut self, ctx: &egui::Context, session: Session) {
        self.sessions.retain(|s| s.work != session.work);
        self.sessions.push(session);
        while self.sessions.len() > MAX_SESSIONS {
            self.sessions.remove(0);
        }
        ctx.request_repaint_after(POLL_EVERY);
    }

    fn prepare_external(&mut self, state: &AppState, loc: &Located) -> Result<(PathBuf, Target), String> {
        match loc.kind {
            Kind::Missing => Err(tf!("vtf.err.not_found", loc.name)),
            Kind::Disk => {
                let path = loc.disk.clone().ok_or_else(|| tf!("vtf.err.not_found", loc.name))?;
                let bak = crate::matedit::backup_path(&path);
                if !bak.exists() {
                    let _ = std::fs::copy(&path, &bak);
                }
                Ok((path.clone(), Target { path, name: loc.name.clone() }))
            }
            Kind::Archive | Kind::Generated => {
                let dest = store::custom_target(state, self.save.dest, &loc.name)?;
                let bytes = match (&loc.bytes, &loc.image_file) {
                    (Some(b), _) => b.clone(),
                    (None, Some(file)) => {
                        let img = Self::read_image(file)?;
                        hp_vtf::encode(&img, &hp_vtf::EncodeOptions::default()).map_err(|e| e.to_string())?
                    }
                    _ => return Err(tf!("vtf.err.not_found", loc.name)),
                };
                let work = external::work_file(&loc.name);
                if let Some(dir) = work.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                }
                std::fs::write(&work, bytes).map_err(|e| format!("{}: {e}", work.display()))?;
                Ok((work, dest))
            }
        }
    }

    fn poll_sessions(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if self.sessions.is_empty() {
            return;
        }
        ctx.request_repaint_after(POLL_EVERY);
        if self.last_poll.is_some_and(|t| t.elapsed() < POLL_EVERY) {
            return;
        }
        self.last_poll = Some(Instant::now());
        let mut done: Vec<SyncDone> = Vec::new();
        for s in &mut self.sessions {
            if s.changed() {
                let copied = s.copy_back();
                s.error = copied.as_ref().err().cloned();
                let link = if copied.is_ok() { s.link.take() } else { None };
                done.push((s.texture.clone(), s.dest.clone(), s.materials.clone(), copied, link));
            }
        }
        for (texture, dest, materials, result, link) in done {
            if let Some((param, name)) = link {
                crate::shared::post_texture_link(ctx, &param, &name);
            }
            match result {
                Ok(copied) => {
                    if copied || !texture.is_empty() {
                        store::refresh(state, &dest, &texture, &materials);
                    }
                    if let Some(l) = self.loaded.as_mut()
                        && !l.doc.is_dirty()
                        && matches!(&l.origin, Origin::Texture(n) if *n == texture)
                    {
                        let loc_name = l.label.clone();
                        if let Some(fs) = state.assets.fs().cloned() {
                            let loc = store::locate(&fs, &loc_name);
                            if let Some(b) = &loc.bytes
                                && let Ok(doc) = VtfDocument::from_bytes(b)
                            {
                                l.doc = doc;
                                l.located = Some(loc);
                                l.rev += 1;
                            }
                        }
                    }
                    state.notify(Severity::Success, tf!("vtf.status.external_synced", texture));
                }
                Err(e) => state.notify(Severity::Error, tf!("vtf.err.sync", texture, e)),
            }
        }
    }

    pub fn stop_session(&mut self, index: usize) {
        if index < self.sessions.len() {
            self.sessions.remove(index);
        }
    }

    pub fn reload(&mut self, state: &mut AppState) {
        let Some(l) = self.loaded.as_ref() else { return };
        let (origin, materials, param) = (l.origin.clone(), l.materials.clone(), l.param.clone());
        match origin {
            Origin::Texture(name) => self.open_texture(state, &name, materials, param),
            Origin::File(path) => self.open_file(state, &path),
            Origin::New => {}
        }
    }
}

fn hp_vtf_safe_name(label: &str) -> String {
    let cleaned: String = label
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    cleaned.trim_matches('_').to_owned()
}

pub(crate) fn face_suffix(face: usize) -> &'static str {
    ["rt", "lf", "bk", "ft", "up", "dn", "sph"].get(face).copied().unwrap_or("face")
}

#[cfg(test)]
mod tests;
