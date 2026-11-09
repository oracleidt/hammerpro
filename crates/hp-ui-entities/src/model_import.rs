use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use egui::{RichText, Ui};
use glam::DVec3;
use hp_app::{AppState, Severity, t, tf, theme};
use hp_meshio::smd::Collision;
use hp_meshio::studio::{StudioError, StudioOptions};
use hp_meshio::{Alpha, ImportOptions, Model, UpAxis};

pub mod batch;
mod batch_ui;
pub use batch_ui::{ModelBatchDialog, Placement};
mod params;

pub const IMPORT_FOLDER: &str = "hammerpro";

const TINY: f64 = 2.0;
const HUGE: f64 = 4096.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Up {
    Auto,
    Y,
    Z,
}

#[derive(Debug)]
struct Prepared {
    model: String,
    textures: usize,
    pieces: usize,
    body_parts: usize,
    lod_triangles: Vec<usize>,
    bounds: (DVec3, DVec3),
    warnings: Vec<String>,
    report: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    pub model: String,
    pub class: &'static str,
    pub bounds: (DVec3, DVec3),
    pub place: bool,
}

enum Stage {
    Idle,
    Picking(Receiver<Option<PathBuf>>),
    Reading(PathBuf, Receiver<Result<Model, String>>),
    Ready(PathBuf, Box<Model>),
    Writing(String, Receiver<Result<Prepared, String>>),
}

pub struct ModelImportDialog {
    stage: Stage,
    name: String,
    up: Up,
    scale: f64,
    convert_units: bool,
    center: bool,
    static_prop: bool,
    surface_prop: String,
    mass: f64,
    collision: Collision,
    place: bool,
    error: Option<String>,
    preview: Option<(ImportOptions, Model)>,
    writing_static: bool,
    imported: Option<Imported>,
    confirm: bool,
}

impl Default for ModelImportDialog {
    fn default() -> Self {
        Self {
            stage: Stage::Idle,
            name: String::new(),
            up: Up::Auto,
            scale: 1.0,
            convert_units: true,
            center: true,
            static_prop: true,
            surface_prop: "default".to_owned(),
            mass: 20.0,
            collision: Collision::Pieces,
            place: true,
            error: None,
            preview: None,
            writing_static: true,
            imported: None,
            confirm: false,
        }
    }
}

pub fn clean_name(stem: &str) -> String {
    let s: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .chars()
        .take(48)
        .collect();
    if s.is_empty() { "model".to_owned() } else { s }
}

pub fn sandbox_content_dir() -> PathBuf {
    std::env::temp_dir().join("hammerpro_sandbox_content")
}

impl ModelImportDialog {
    pub fn busy(&self) -> bool {
        matches!(self.stage, Stage::Picking(_) | Stage::Reading(..) | Stage::Writing(..))
    }

    pub fn open(&mut self, ctx: &egui::Context) {
        if self.busy() {
            return;
        }
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            let file =
                rfd::FileDialog::new().add_filter(t!("entities.mi_filter"), &["fbx", "obj", "gltf", "glb"]).pick_file();
            let _ = tx.send(file);
            repaint.request_repaint();
        });
        self.stage = Stage::Picking(rx);
        self.error = None;
    }

    pub fn open_file(&mut self, ctx: &egui::Context, path: PathBuf) {
        if self.busy() {
            return;
        }
        self.name = clean_name(&path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
        let (tx, rx) = channel();
        let (repaint, p) = (ctx.clone(), path.clone());
        std::thread::spawn(move || {
            let _ = tx.send(hp_meshio::read(&p).map_err(|e| e.to_string()));
            repaint.request_repaint();
        });
        self.stage = Stage::Reading(path, rx);
        self.error = None;
        self.preview = None;
    }

    pub fn set_scale(&mut self, scale: f64) {
        if scale.is_finite() && scale > 0.0 {
            self.scale = scale;
        }
    }

    pub fn confirm(&mut self) {
        self.confirm = true;
    }

    pub fn take_imported(&mut self) -> Option<Imported> {
        self.imported.take()
    }

    fn options(&self) -> ImportOptions {
        ImportOptions {
            up: match self.up {
                Up::Auto => None,
                Up::Y => Some(UpAxis::Y),
                Up::Z => Some(UpAxis::Z),
            },
            scale: self.scale,
            convert_units: self.convert_units,
            center: self.center,
        }
    }

    fn source(&mut self, model: &Model) -> &Model {
        let opts = self.options();
        if self.preview.as_ref().is_none_or(|(o, _)| *o != opts) {
            self.preview = Some((opts, model.to_source(&opts)));
        }
        &self.preview.as_ref().expect("computed").1
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.poll(ctx, state);
        let busy_text = match &self.stage {
            Stage::Reading(path, _) => {
                Some(tf!("entities.mi_reading", path.file_name().map(|f| f.to_string_lossy()).unwrap_or_default()))
            }
            Stage::Writing(name, _) => Some(tf!("entities.mi_writing", format!("models/{IMPORT_FOLDER}/{name}.mdl"))),
            _ => None,
        };
        if let Some(text) = busy_text {
            let reading = matches!(self.stage, Stage::Reading(..));
            let mut open = true;
            let mut window = hp_app::chrome::Window::new(t!("entities.mi_title"))
                .id(egui::Id::unique("hp_model_import_busy"))
                .collapsible(false)
                .resizable(false);
            if reading {
                window = window.open(&mut open);
            }
            let resp = window.show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(text);
                });
            });
            let escape =
                reading && crate::dialogs::escape_closes(ctx, resp.as_ref().map(|r| r.response.layer_id), false);
            if reading && (!open || escape) {
                self.stage = Stage::Idle;
            }
            return;
        }
        let Stage::Ready(path, model) = std::mem::replace(&mut self.stage, Stage::Idle) else { return };
        let mut open = true;
        let mut start = false;
        let response = hp_app::chrome::Window::new(t!("entities.mi_title"))
            .id(egui::Id::unique("hp_model_import"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .show(ctx, |ui| start = self.ui(ui, state, &path, &model));
        if crate::dialogs::escape_closes(ctx, response.as_ref().map(|r| r.response.layer_id), false) {
            open = false;
        }
        if !open {
            return;
        }
        if !(start && self.start(ctx, state, &model)) {
            self.stage = Stage::Ready(path, model);
        }
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, path: &Path, model: &Model) -> bool {
        let (tris, size) = {
            let source = self.source(model);
            (source.triangles.len(), source.bounds().map_or(DVec3::ZERO, |(lo, hi)| hi - lo))
        };
        ui.label(RichText::new(path.display().to_string()).small().weak());
        ui.label(tf!(
            "entities.mi_summary",
            tris,
            model.materials.len(),
            format!("{:.1} x {:.1} x {:.1}", size.x, size.y, size.z)
        ));
        if tris > 65_000 {
            ui.colored_label(theme::warning(), t!("entities.mi_too_many"));
        }
        if size.max_element() < TINY {
            ui.colored_label(theme::warning(), t!("entities.mi_tiny"));
        } else if size.max_element() > HUGE {
            ui.colored_label(theme::warning(), t!("entities.mi_huge"));
        }
        egui::Grid::new("hp_mi_grid").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label(t!("entities.mi_name"));
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("models/{IMPORT_FOLDER}/")).weak());
                ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(180.0));
                ui.label(RichText::new(".mdl").weak());
            });
            ui.end_row();
            ui.label(t!("entities.mi_kind"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.static_prop, true, "prop_static");
                ui.radio_value(&mut self.static_prop, false, t!("entities.mi_physics"));
            });
            ui.end_row();
            ui.label(t!("entities.mi_scale"));
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.scale).range(0.001..=1000.0).speed(0.01).max_decimals(3));
                ui.checkbox(&mut self.convert_units, t!("entities.mi_units"))
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
            ui.checkbox(&mut self.center, t!("entities.mi_center"));
            ui.end_row();
            ui.label(t!("entities.surface_prop"));
            egui::ComboBox::from_id_salt("hp_mi_surface").selected_text(&self.surface_prop).height(360.0).show_ui(
                ui,
                |ui| {
                    for sp in SURFACES {
                        ui.selectable_value(&mut self.surface_prop, (*sp).to_owned(), *sp);
                    }
                },
            );
            ui.end_row();
            ui.label(t!("entities.mi_collision"));
            ui.horizontal(|ui| {
                ui.radio_value(&mut self.collision, Collision::Pieces, t!("entities.mi_coll_pieces"))
                    .on_hover_text(t!("entities.mi_coll_pieces_tip"));
                ui.radio_value(&mut self.collision, Collision::Box, t!("entities.mi_coll_box"));
                ui.radio_value(&mut self.collision, Collision::None, t!("entities.mi_coll_none"));
            });
            ui.end_row();
            if !self.static_prop {
                ui.label(t!("entities.mass"));
                ui.add(egui::DragValue::new(&mut self.mass).range(0.1..=50000.0).suffix(" kg"));
                ui.end_row();
            }
        });
        if !self.static_prop && self.collision == Collision::None {
            ui.colored_label(theme::warning(), t!("entities.mi_physics_needs_collision"));
        }
        if hp_app::widgets::model_lod_ui(ui, &mut state.settings.lab.model_lod) {
            state.settings_dirty = true;
        }
        ui.checkbox(&mut self.place, t!("entities.mi_place"));
        if ui
            .checkbox(&mut state.settings.lab.model_keep_params, t!("entities.mi_keep_params"))
            .on_hover_text(t!("entities.mi_keep_params_tip"))
            .changed()
        {
            state.settings_dirty = true;
        }
        ui.add_space(4.0);
        ui.label(RichText::new(t!("entities.mi_materials")).strong());
        for (i, m) in model.materials.iter().enumerate() {
            if !model.triangles.iter().any(|t| t.material == i) {
                continue;
            }
            let texture = match (&m.texture, &m.embedded) {
                (Some(p), _) => p.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default(),
                (None, Some(_)) => t!("entities.mi_embedded").to_owned(),
                (None, None) => t!("entities.mi_no_texture").to_owned(),
            };
            let color = if m.texture.is_some() || m.embedded.is_some() { theme::success() } else { theme::warning() };
            ui.horizontal(|ui| {
                ui.label(&m.name);
                ui.colored_label(color, texture);
            });
        }
        if let Some(e) = &self.error {
            ui.colored_label(theme::error(), e);
        }
        ui.separator();
        let exists = content_root(state).is_some_and(|g| {
            g.join("models").join(IMPORT_FOLDER).join(format!("{}.mdl", clean_name(&self.name))).is_file()
        });
        if exists {
            ui.colored_label(theme::warning(), t!("entities.mi_exists"));
        }
        ui.label(RichText::new(t!("entities.mi_where")).small().weak());
        let enabled = !self.name.trim().is_empty();
        let clicked =
            ui.add_enabled(enabled, egui::Button::new(RichText::new(t!("entities.mi_import")).strong())).clicked();
        clicked || (enabled && std::mem::take(&mut self.confirm))
    }

    fn start(&mut self, ctx: &egui::Context, state: &mut AppState, model: &Model) -> bool {
        let Some(root) = content_root(state) else {
            self.error = Some(t!("entities.mi_no_game").to_owned());
            return false;
        };
        if hp_compile::sandbox::is_sandboxed() {
            state.log(Severity::Info, tf!("entities.mi_sandboxed", root.display()));
        }
        let name = clean_name(&self.name);
        self.name.clone_from(&name);
        let mut source = self.source(model).clone();
        let keep = state.settings.lab.model_keep_params;
        if !keep {
            source.bake_uv_transforms();
        }
        let opts = StudioOptions {
            model: format!("{IMPORT_FOLDER}/{name}"),
            cdmaterials: format!("models/{IMPORT_FOLDER}/{name}"),
            static_prop: self.static_prop,
            surface_prop: self.surface_prop.clone(),
            mass: self.mass,
            collision: self.collision,
            lod: lod_options(state.settings.lab.model_lod),
        };
        self.writing_static = self.static_prop;
        self.error = None;
        let (tx, rx) = channel();
        let repaint = ctx.clone();
        let worker_name = name.clone();
        std::thread::spawn(move || {
            let _ = tx.send(prepare(&source, &opts, &worker_name, &root, keep));
            repaint.request_repaint();
        });
        self.stage = Stage::Writing(name, rx);
        true
    }

    fn poll(&mut self, ctx: &egui::Context, state: &mut AppState) {
        match std::mem::replace(&mut self.stage, Stage::Idle) {
            Stage::Picking(rx) => match rx.try_recv() {
                Ok(Some(path)) => self.open_file(ctx, path),
                Ok(None) | Err(TryRecvError::Disconnected) => {}
                Err(TryRecvError::Empty) => self.stage = Stage::Picking(rx),
            },
            Stage::Reading(path, rx) => match rx.try_recv() {
                Ok(Ok(model)) => self.stage = Stage::Ready(path, Box::new(model)),
                Ok(Err(e)) => state.notify(Severity::Error, tf!("entities.mi_read_failed", e)),
                Err(TryRecvError::Empty) => self.stage = Stage::Reading(path, rx),
                Err(TryRecvError::Disconnected) => {}
            },
            Stage::Writing(name, rx) => match rx.try_recv() {
                Ok(Ok(p)) => self.finish(state, p),
                Ok(Err(e)) => state.notify(Severity::Error, tf!("entities.mi_failed", e)),
                Err(TryRecvError::Empty) => self.stage = Stage::Writing(name, rx),
                Err(TryRecvError::Disconnected) => {}
            },
            other => self.stage = other,
        }
    }

    fn finish(&mut self, state: &mut AppState, p: Prepared) {
        for w in &p.warnings {
            state.log(Severity::Warning, w.clone());
        }
        for r in &p.report {
            state.log(Severity::Info, r.clone());
        }
        remount_after_write(state);
        let class = if self.writing_static { "prop_static" } else { "prop_physics" };
        state.current_model = Some(p.model.clone());
        state.current_model_skin = 0;
        state.current_entity_class = class.to_owned();
        if p.body_parts > 1 {
            state.log(Severity::Info, tf!("entities.mi_split", p.body_parts));
        }
        if p.lod_triangles.len() > 1 {
            let counts: Vec<String> = p.lod_triangles.iter().map(ToString::to_string).collect();
            state.log(Severity::Info, tf!("entities.mi_lods", p.lod_triangles.len() - 1, counts.join(" / ")));
        }
        state.notify(Severity::Success, tf!("entities.mi_done", p.model.clone(), p.textures, p.pieces));
        self.imported = Some(Imported { model: p.model, class, bounds: p.bounds, place: self.place });
    }
}

pub fn remount_after_write(state: &mut AppState) {
    if hp_compile::sandbox::is_sandboxed() {
        let dir = sandbox_content_dir();
        let dirs = &mut state.settings.game.custom_texture_dirs;
        if !dirs.iter().any(|d| d.path == dir) {
            dirs.push(hp_app::settings::CustomDirSetting {
                path: dir,
                prefix: String::new(),
                enabled: true,
                include_models: true,
            });
        }
    }
    state.assets.remount(&state.settings);
}

pub fn lod_options(s: hp_app::settings::ModelLodSettings) -> hp_meshio::lod::LodOptions {
    let s = s.clamped();
    hp_meshio::lod::LodOptions {
        enabled: s.enabled,
        levels: s.levels as usize,
        reduction: f64::from(s.reduction),
        distances: s.distances,
    }
}

pub fn content_root(state: &AppState) -> Option<PathBuf> {
    if hp_compile::sandbox::is_sandboxed() {
        return Some(sandbox_content_dir());
    }
    state.assets.fs().and_then(|f| f.game_dir().map(Path::to_path_buf))
}

const SURFACES: &[&str] = &[
    "default",
    "concrete",
    "metal",
    "solidmetal",
    "metal_box",
    "metalpanel",
    "wood",
    "wood_crate",
    "wood_furniture",
    "plastic",
    "plastic_barrel",
    "glass",
    "rock",
    "dirt",
    "grass",
    "gravel",
    "brick",
    "tile",
    "carpet",
    "cardboard",
    "paper",
    "rubber",
    "flesh",
    "ice",
    "porcelain",
    "computer",
    "pottery",
];

const COLOR_TEXTURE: u32 = 8;

fn srgb(v: f64) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let s = if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
    (s * 255.0).round() as u8
}

fn color_image(c: [f64; 4]) -> hp_vtf::RgbaImage {
    let px = [srgb(c[0]), srgb(c[1]), srgb(c[2]), (c[3].clamp(0.0, 1.0) * 255.0).round() as u8];
    hp_vtf::RgbaImage {
        width: COLOR_TEXTURE,
        height: COLOR_TEXTURE,
        pixels: px.repeat((COLOR_TEXTURE * COLOR_TEXTURE) as usize),
    }
}

fn material_image(m: &hp_meshio::Material, warnings: &mut Vec<String>) -> Option<hp_vtf::RgbaImage> {
    let (bytes, ext) = match (&m.texture, &m.embedded) {
        (Some(p), _) => match std::fs::read(p) {
            Ok(b) => (b, p.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()),
            Err(e) => {
                warnings.push(tf!("entities.mi_texture_failed", m.name, e.to_string()));
                return None;
            }
        },
        (None, Some((b, ext))) => (b.clone(), ext.clone()),
        (None, None) => return None,
    };
    match hp_vtf::load_image_file(&bytes, &ext) {
        Ok(img) => Some(img),
        Err(e) => {
            warnings.push(tf!("entities.mi_texture_failed", m.name, e.to_string()));
            None
        }
    }
}

fn vector(c: &[f64]) -> String {
    format!("[{:.3} {:.3} {:.3}]", c[0], c[1], c[2])
}

fn prepare(model: &Model, opts: &StudioOptions, name: &str, root: &Path, keep: bool) -> Result<Prepared, String> {
    let mat_dir = root.join("materials").join("models").join(IMPORT_FOLDER).join(name);
    let model_dir = root.join("models").join(IMPORT_FOLDER);
    for d in [&mat_dir, &model_dir] {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let mut names: Vec<String> = Vec::with_capacity(model.materials.len());
    let mut warnings = Vec::new();
    let mut textures = 0;
    let mut report: Vec<String> = Vec::new();
    for (i, m) in model.materials.iter().enumerate() {
        let mut file = hp_meshio::smd::material_file_name(&m.name, i);
        while names.contains(&file) {
            file.push('_');
        }
        if model.triangles.iter().any(|t| t.material == i) {
            let tex_path = format!("models/{IMPORT_FOLDER}/{name}/{file}");
            let image = material_image(m, &mut warnings);
            if image.is_none() && m.texture.is_none() && m.embedded.is_none() {
                warnings.push(tf!("entities.mi_material_untextured", m.name));
            }
            let textured = image.is_some();
            let img = image.unwrap_or_else(|| color_image(m.color));
            let enc = hp_vtf::EncodeOptions {
                format: hp_vtf::EncodeFormat::Auto,
                mipmaps: true,
                quality: hp_vtf::EncodeQuality::Normal,
                ..hp_vtf::EncodeOptions::default()
            };
            let vtf = hp_vtf::encode(&img, &enc).map_err(|e| format!("{}: {e}", m.name))?;
            std::fs::write(mat_dir.join(format!("{file}.vtf")), vtf).map_err(|e| e.to_string())?;
            textures += usize::from(textured);
            let (mut binary, mut blended) = (false, false);
            for p in img.pixels.chunks_exact(4) {
                match p[3] {
                    255 => {}
                    0 => binary = true,
                    _ => blended = true,
                }
            }
            let has_alpha = binary || blended;
            let mut vmt = hp_vmt::ImageMaterialOptions {
                shader: "VertexLitGeneric".to_owned(),
                nocull: m.double_sided,
                ..Default::default()
            };
            match m.alpha {
                Alpha::Opaque => {}
                Alpha::Mask => vmt.alphatest = has_alpha,
                Alpha::Blend => vmt.translucent = has_alpha || m.color[3] < 0.999,
                Alpha::Auto => {
                    vmt.alphatest = binary && !blended;
                    vmt.translucent = blended || m.color[3] < 0.999;
                }
            }
            let mut mat = hp_vmt::for_image(&tex_path, &vmt);
            if textured && m.color[..3].iter().any(|c| (c - 1.0).abs() > 0.004) {
                mat.params.push(("$color2".to_owned(), vector(&m.color)));
            }
            if textured && m.color[3] < 0.999 && m.alpha != Alpha::Opaque {
                mat.params.push(("$alpha".to_owned(), format!("{:.3}", m.color[3])));
            }
            if keep {
                let built = params::build(m, &tex_path, &file, &mat_dir, textured, &mut warnings)?;
                mat.params.extend(built.params.iter().cloned());
                textures += built.textures;
                report.push(params::report(&m.name, &built));
            }
            std::fs::write(mat_dir.join(format!("{file}.vmt")), hp_vmt::write(&mat)).map_err(|e| e.to_string())?;
        }
        names.push(file);
    }
    let files = hp_meshio::studio::compile(model, &names, opts).map_err(|e| match e {
        StudioError::Empty => t!("entities.mi_err_empty").to_owned(),
        StudioError::TooManyMaterials(n) => tf!("entities.mi_err_materials", n),
        StudioError::TooLarge => t!("entities.mi_err_large").to_owned(),
    })?;
    let base = model_dir.join(name);
    let write = |ext: &str, data: &[u8]| {
        let p = base.with_extension(ext);
        std::fs::write(&p, data).map_err(|e| format!("{}: {e}", p.display()))
    };
    write("vvd", &files.vvd)?;
    write("dx90.vtx", &files.vtx)?;
    match &files.phy {
        Some(phy) => write("phy", phy)?,
        None => {
            let _ = std::fs::remove_file(base.with_extension("phy"));
        }
    }
    for ext in ["dx80.vtx", "sw.vtx", "vtx", "ani"] {
        let _ = std::fs::remove_file(base.with_extension(ext));
    }
    write("mdl", &files.mdl)?;
    let bounds = model.bounds().unwrap_or((DVec3::ZERO, DVec3::ZERO));
    Ok(Prepared {
        model: format!("models/{IMPORT_FOLDER}/{name}.mdl"),
        textures,
        pieces: files.convex_pieces,
        body_parts: files.body_parts,
        lod_triangles: files.lod_triangles.clone(),
        bounds,
        warnings,
        report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_reads_then_shows_the_options() {
        crate::register_translations();
        let dir = std::env::temp_dir().join("hp_model_import_dialog");
        let _ = std::fs::create_dir_all(&dir);
        let obj = dir.join("Small Crate.obj");
        std::fs::write(&obj, "v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        let mut state = hp_app::testing::test_state();
        let mut d = ModelImportDialog::default();
        let ctx = egui::Context::default();
        d.open_file(&ctx, obj);
        assert_eq!(d.name, "small_crate");
        for _ in 0..200 {
            hp_app::testing::run_ctx(&mut state, |ctx, state| d.show(ctx, state));
            if matches!(d.stage, Stage::Ready(..)) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(matches!(d.stage, Stage::Ready(..)), "the options are shown");
        hp_app::testing::run_ctx(&mut state, |ctx, state| d.show(ctx, state));
        assert!(matches!(d.stage, Stage::Ready(..)), "still open");
        assert!(!d.busy());
    }

    #[test]
    fn names_and_prepared_files() {
        crate::register_translations();
        assert_eq!(clean_name("My Chair (v2)"), "my_chair__v2");
        assert_eq!(clean_name("!!!"), "model");
        let root = std::env::temp_dir().join("hp_model_import_test");
        let _ = std::fs::remove_dir_all(&root);
        let game = root.join("game");
        std::fs::create_dir_all(&root).unwrap();
        let mut px = [255u8, 0, 0, 255].repeat(16);
        px[3] = 0;
        image::save_buffer(root.join("red.png"), &px, 4, 4, image::ColorType::Rgba8).unwrap();
        std::fs::write(
            root.join("in.mtl"),
            "newmtl Red Paint\nmap_Kd red.png\nnewmtl blue\nKd 0 0 1\nnewmtl gone\nmap_Kd missing.png\n",
        )
        .unwrap();
        std::fs::write(
            root.join("in.obj"),
            "mtllib in.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\nusemtl Red Paint\nf 1 2 3\nusemtl blue\nf 1 2 4\nusemtl gone\nf 1 3 4\n",
        )
        .unwrap();
        let model = hp_meshio::read(&root.join("in.obj")).expect("read").to_source(&ImportOptions::default());
        let opts = StudioOptions {
            model: "hammerpro/tri".to_owned(),
            cdmaterials: "models/hammerpro/tri".to_owned(),
            static_prop: true,
            surface_prop: "wood".to_owned(),
            mass: 10.0,
            collision: Collision::Box,
            lod: hp_meshio::lod::LodOptions::default(),
        };
        let p = prepare(&model, &opts, "tri", &game, true).expect("prepare");
        assert_eq!(p.model, "models/hammerpro/tri.mdl");
        assert_eq!(p.textures, 1);
        assert_eq!(p.pieces, 1);
        assert!(p.warnings.iter().any(|w| w.contains("gone")), "{:?}", p.warnings);
        let mats = game.join("materials/models/hammerpro/tri");
        for f in ["red_paint", "blue", "gone"] {
            assert!(mats.join(format!("{f}.vtf")).is_file(), "{f}.vtf");
        }
        let vmt = std::fs::read_to_string(mats.join("red_paint.vmt")).unwrap();
        assert!(vmt.contains("VertexLitGeneric") && vmt.contains("models/hammerpro/tri/red_paint"), "{vmt}");
        assert!(vmt.contains("$alphatest"), "cut out alpha: {vmt}");
        let models = game.join("models/hammerpro");
        let read = |ext: &str| std::fs::read(models.join(format!("tri.{ext}"))).expect(ext);
        let m = hp_mdl::load(&read("mdl"), &read("vvd"), &read("dx90.vtx"), &hp_mdl::LoadOptions::default())
            .expect("the editor loads the model");
        assert!(m.is_static_prop);
        assert_eq!(m.materials, ["red_paint", "blue", "gone"]);
        assert_eq!(m.triangle_count(), 3);
        assert!(hp_mdl::parse_phy(&read("phy")).is_ok());
        assert_eq!(srgb(1.0), 255);
        assert_eq!(srgb(0.0), 0);
        assert_eq!(srgb(0.5), 188);
        let _ = std::fs::remove_dir_all(&root);
    }

    fn opts_for(name: &str) -> StudioOptions {
        StudioOptions {
            model: format!("hammerpro/{name}"),
            cdmaterials: format!("models/hammerpro/{name}"),
            static_prop: true,
            surface_prop: "default".to_owned(),
            mass: 10.0,
            collision: Collision::Box,
            lod: hp_meshio::lod::LodOptions::default(),
        }
    }

    fn png(path: &Path, px: [u8; 4]) {
        image::save_buffer(path, &px.repeat(16), 4, 4, image::ColorType::Rgba8).unwrap();
    }

    #[test]
    fn obj_material_parameters_reach_the_vmt() {
        crate::register_translations();
        let root = std::env::temp_dir().join("hp_model_import_obj_params");
        let _ = std::fs::remove_dir_all(&root);
        let game = root.join("game");
        std::fs::create_dir_all(&root).unwrap();
        png(&root.join("d.png"), [200, 100, 50, 255]);
        png(&root.join("n.png"), [128, 128, 255, 255]);
        png(&root.join("e.png"), [255, 200, 0, 255]);
        std::fs::write(
            root.join("in.mtl"),
            "newmtl shiny\nmap_Kd -s 2 3 1 d.png\nmap_Bump n.png\nKs 0.9 0.9 0.9\nNs 300\nKe 1 1 1\nmap_Ke e.png\nmap_Pr x.png\n",
        )
        .unwrap();
        std::fs::write(root.join("in.obj"), "mtllib in.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nusemtl shiny\nf 1 2 3\n")
            .unwrap();
        let model = hp_meshio::read(&root.join("in.obj")).expect("read").to_source(&ImportOptions::default());
        let p = prepare(&model, &opts_for("o"), "o", &game, true).expect("prepare");
        let mats = game.join("materials/models/hammerpro/o");
        let vmt = std::fs::read_to_string(mats.join("shiny.vmt")).unwrap();
        for key in [
            "$bumpmap",
            "$phong",
            "$phongexponent",
            "$selfillum",
            "$selfillummask",
            "$basetexturetransform",
            "$bumptransform",
        ] {
            assert!(vmt.contains(key), "{key} in {vmt}");
        }
        assert!(vmt.contains("models/hammerpro/o/shiny_n") && vmt.contains("shiny_illum"), "{vmt}");
        for f in ["shiny.vtf", "shiny_n.vtf", "shiny_illum.vtf"] {
            assert!(mats.join(f).is_file(), "{f}");
        }
        assert_eq!(p.report.len(), 1);
        assert!(p.report[0].contains("shiny") && p.report[0].contains("$phong"), "{:?}", p.report);
        let off = prepare(&model, &opts_for("o2"), "o2", &game, false).expect("prepare");
        let plain = std::fs::read_to_string(game.join("materials/models/hammerpro/o2/shiny.vmt")).unwrap();
        assert!(!plain.contains("$bumpmap") && !plain.contains("$phong") && !plain.contains("transform"), "{plain}");
        assert!(off.report.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn gltf_material_parameters_reach_the_vmt() {
        crate::register_translations();
        let root = std::env::temp_dir().join("hp_model_import_gltf_params");
        let _ = std::fs::remove_dir_all(&root);
        let game = root.join("game");
        std::fs::create_dir_all(&root).unwrap();
        png(&root.join("c.png"), [90, 90, 90, 255]);
        png(&root.join("n.png"), [128, 128, 255, 255]);
        png(&root.join("mr.png"), [0, 51, 255, 255]);
        let mut bin = Vec::new();
        for v in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        std::fs::write(root.join("t.bin"), &bin).unwrap();
        let json = r#"{"asset":{"version":"2.0"},"scenes":[{"nodes":[0]}],"nodes":[{"mesh":0}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
            "buffers":[{"byteLength":36,"uri":"t.bin"}],
            "bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],
            "images":[{"uri":"c.png"},{"uri":"n.png"},{"uri":"mr.png"}],
            "textures":[{"source":0},{"source":1},{"source":2}],
            "materials":[{"name":"metal","pbrMetallicRoughness":{"metallicFactor":1,"roughnessFactor":1,
              "baseColorTexture":{"index":0,"extensions":{"KHR_texture_transform":{"scale":[4,4]}}},
              "metallicRoughnessTexture":{"index":2}},"normalTexture":{"index":1},
              "emissiveFactor":[0.2,0.4,0.6]}]}"#;
        std::fs::write(root.join("m.gltf"), json).unwrap();
        let model = hp_meshio::read(&root.join("m.gltf")).expect("read").to_source(&ImportOptions::default());
        prepare(&model, &opts_for("g"), "g", &game, true).expect("prepare");
        let vmt = std::fs::read_to_string(game.join("materials/models/hammerpro/g/metal.vmt")).unwrap();
        for key in ["$bumpmap", "\"$phong\" \"1\"", "$envmap", "$selfillumtint", "scale 4.0000 4.0000"] {
            assert!(vmt.contains(key), "{key} in {vmt}");
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
