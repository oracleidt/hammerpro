use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use glam::DVec3;
use hp_app::i18n::TrTable;
use hp_app::{t, tf};
use hp_meshio::smd::Collision;
use hp_meshio::studio::StudioOptions;
use hp_meshio::{ImportOptions, Model};

use super::{HUGE, IMPORT_FOLDER, Prepared, TINY, clean_name, prepare};

pub const MAX_FILES: usize = 5000;
pub const MAX_DEPTH: usize = 24;
pub const MAX_THREADS: usize = 16;
pub const HEAVY_TRIANGLES: usize = 65_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Formats {
    pub obj: bool,
    pub fbx: bool,
    pub gltf: bool,
}

impl Default for Formats {
    fn default() -> Self {
        Self { obj: true, fbx: true, gltf: true }
    }
}

impl Formats {
    pub fn accepts(self, path: &Path) -> bool {
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        match ext.as_str() {
            "obj" => self.obj,
            "fbx" => self.fbx,
            "gltf" | "glb" => self.gltf,
            _ => false,
        }
    }

    pub fn parse(list: &str) -> Result<Self, String> {
        let mut f = Formats { obj: false, fbx: false, gltf: false };
        for word in list.split([',', ';', ' ']).map(str::trim).filter(|w| !w.is_empty()) {
            match word.to_ascii_lowercase().as_str() {
                "obj" => f.obj = true,
                "fbx" => f.fbx = true,
                "gltf" | "glb" => f.gltf = true,
                other => return Err(format!("unknown format {other}: obj, fbx or gltf")),
            }
        }
        if f == (Formats { obj: false, fbx: false, gltf: false }) {
            return Err("no format selected: obj, fbx, gltf".to_owned());
        }
        Ok(f)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConvertOptions {
    pub import: ImportOptions,
    pub static_prop: bool,
    pub surface_prop: String,
    pub mass: f64,
    pub collision: Collision,
    pub lod: hp_meshio::lod::LodOptions,
    pub keep_params: bool,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            import: ImportOptions::default(),
            static_prop: true,
            surface_prop: "default".to_owned(),
            mass: 20.0,
            collision: Collision::Pieces,
            lod: hp_meshio::lod::LodOptions::default(),
            keep_params: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatchOptions {
    pub convert: ConvertOptions,
    pub template: String,
    pub prefix: String,
    pub overwrite: bool,
    pub threads: usize,
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            convert: ConvertOptions::default(),
            template: "{name}".to_owned(),
            prefix: String::new(),
            overwrite: false,
            threads: default_threads(),
        }
    }
}

pub fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get() / 2).clamp(1, 4)
}

pub fn clamp_threads(n: usize) -> usize {
    n.clamp(1, MAX_THREADS)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Converted {
    pub model: String,
    pub triangles: usize,
    pub size: DVec3,
    pub textures: usize,
    pub pieces: usize,
    pub body_parts: usize,
    pub lod_triangles: Vec<usize>,
    pub bounds: (DVec3, DVec3),
    pub warnings: Vec<String>,
    pub report: Vec<String>,
}

pub fn model_warnings(triangles: usize, size: DVec3) -> Vec<String> {
    let mut out = Vec::new();
    if triangles > HEAVY_TRIANGLES {
        out.push(t!("entities.mi_too_many").to_owned());
    }
    if size.max_element() < TINY {
        out.push(t!("entities.mi_tiny").to_owned());
    } else if size.max_element() > HUGE {
        out.push(t!("entities.mi_huge").to_owned());
    }
    out
}

pub fn convert_model(model: &Model, name: &str, opts: &ConvertOptions, root: &Path) -> Result<Converted, String> {
    let name = clean_name(name);
    let mut source = model.to_source(&opts.import);
    if !opts.keep_params {
        source.bake_uv_transforms();
    }
    let triangles = source.triangles.len();
    let size = source.bounds().map_or(DVec3::ZERO, |(lo, hi)| hi - lo);
    let studio = StudioOptions {
        model: format!("{IMPORT_FOLDER}/{name}"),
        cdmaterials: format!("models/{IMPORT_FOLDER}/{name}"),
        static_prop: opts.static_prop,
        surface_prop: opts.surface_prop.clone(),
        mass: opts.mass,
        collision: opts.collision,
        lod: opts.lod,
    };
    let Prepared { model, textures, pieces, body_parts, lod_triangles, bounds, mut warnings, report } =
        prepare(&source, &studio, &name, root, opts.keep_params)?;
    let mut all = model_warnings(triangles, size);
    all.append(&mut warnings);
    Ok(Converted { model, triangles, size, textures, pieces, body_parts, lod_triangles, bounds, warnings: all, report })
}

pub fn convert_file(path: &Path, name: &str, opts: &ConvertOptions, root: &Path) -> Result<Converted, String> {
    let model = hp_meshio::read(path).map_err(|e| e.to_string())?;
    convert_model(&model, name, opts, root)
}

pub fn collect_files(inputs: &[PathBuf], recursive: bool, formats: Formats) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for input in inputs {
        if input.is_dir() {
            walk(input, recursive, formats, 0, &mut found);
        } else if formats.accepts(input) && input.is_file() {
            found.push(input.clone());
        }
        if found.len() >= MAX_FILES {
            break;
        }
    }
    let mut seen = HashSet::new();
    found.retain(|p| seen.insert(p.clone()));
    found.sort_by_key(|p| p.to_string_lossy().to_ascii_lowercase());
    found.truncate(MAX_FILES);
    found
}

fn walk(dir: &Path, recursive: bool, formats: Formats, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        if out.len() >= MAX_FILES {
            return;
        }
        if p.is_dir() {
            if recursive && depth < MAX_DEPTH && !is_link(&p) {
                walk(&p, recursive, formats, depth + 1, out);
            }
        } else if formats.accepts(&p) {
            out.push(p);
        }
    }
}

fn is_link(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
}

pub fn apply_template(template: &str, prefix: &str, path: &Path, index: usize) -> String {
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let folder = path.parent().and_then(Path::file_name).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let template = if template.trim().is_empty() { "{name}" } else { template };
    let text = template
        .replace("{name}", &clean_name(&stem))
        .replace("{folder}", &folder_name(&folder))
        .replace("{index}", &format!("{index:03}"));
    clean_name(&format!("{prefix}{text}"))
}

fn folder_name(folder: &str) -> String {
    let c: String = folder
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
        .collect::<String>()
        .trim_matches('_')
        .to_owned();
    c
}

pub fn unique_names(names: Vec<String>) -> Vec<String> {
    let mut taken: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(names.len());
    for name in names {
        let mut candidate = name.clone();
        let mut n = 2;
        while !taken.insert(candidate.clone()) {
            let suffix = format!("_{n}");
            let keep = name.chars().take(48usize.saturating_sub(suffix.len())).collect::<String>();
            candidate = format!("{keep}{suffix}");
            n += 1;
        }
        out.push(candidate);
    }
    out
}

pub fn names_for(files: &[PathBuf], template: &str, prefix: &str) -> Vec<String> {
    unique_names(files.iter().enumerate().map(|(i, p)| apply_template(template, prefix, p, i + 1)).collect())
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BatchEntry {
    pub path: PathBuf,
    pub name: String,
    pub triangles: usize,
    pub size: DVec3,
    pub materials: usize,
    pub warnings: Vec<String>,
    pub error: Option<String>,
    pub exists: bool,
}

pub fn model_exists(root: &Path, name: &str) -> bool {
    root.join("models").join(IMPORT_FOLDER).join(format!("{name}.mdl")).is_file()
}

pub fn scan(
    files: &[PathBuf],
    opts: &BatchOptions,
    root: Option<&Path>,
    cancel: &AtomicBool,
    done: &AtomicUsize,
) -> Vec<BatchEntry> {
    let names = names_for(files, &opts.template, &opts.prefix);
    let slots: Vec<std::sync::Mutex<Option<BatchEntry>>> = files.iter().map(|_| std::sync::Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..clamp_threads(opts.threads).min(files.len().max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= files.len() || cancel.load(Ordering::SeqCst) {
                        return;
                    }
                    let entry = scan_one(&files[i], &names[i], opts, root);
                    if let Ok(mut slot) = slots[i].lock() {
                        *slot = Some(entry);
                    }
                    done.fetch_add(1, Ordering::SeqCst);
                }
            });
        }
    });
    slots
        .into_iter()
        .enumerate()
        .map(|(i, slot)| {
            slot.into_inner().ok().flatten().unwrap_or_else(|| BatchEntry {
                path: files[i].clone(),
                name: names[i].clone(),
                ..BatchEntry::default()
            })
        })
        .collect()
}

fn scan_one(path: &Path, name: &str, opts: &BatchOptions, root: Option<&Path>) -> BatchEntry {
    let mut entry = BatchEntry {
        path: path.to_path_buf(),
        name: name.to_owned(),
        exists: root.is_some_and(|r| model_exists(r, name)),
        ..BatchEntry::default()
    };
    match hp_meshio::read(path) {
        Ok(model) => {
            let source = model.to_source(&opts.convert.import);
            entry.triangles = source.triangles.len();
            entry.size = source.bounds().map_or(DVec3::ZERO, |(lo, hi)| hi - lo);
            entry.materials = model.materials.len();
            entry.warnings = model_warnings(entry.triangles, entry.size);
            if !opts.convert.static_prop && opts.convert.collision == Collision::None {
                entry.warnings.push(t!("entities.mi_physics_needs_collision").to_owned());
            }
        }
        Err(e) => entry.error = Some(e.to_string()),
    }
    entry
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warning,
    Error,
    Skipped,
}

impl Status {
    pub fn id(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Warning => "warning",
            Status::Error => "error",
            Status::Skipped => "skipped",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileResult {
    pub index: usize,
    pub path: PathBuf,
    pub name: String,
    pub status: Status,
    pub message: String,
    pub warnings: Vec<String>,
    pub converted: Option<Converted>,
}

impl FileResult {
    pub fn model_path(&self) -> Option<&str> {
        self.converted.as_ref().map(|c| c.model.as_str())
    }

    pub fn line(&self) -> String {
        let file = self.path.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
        let label = match self.status {
            Status::Ok => t!("entities.mb_ok"),
            Status::Warning => t!("entities.mb_warning"),
            Status::Error => t!("entities.mb_error"),
            Status::Skipped => t!("entities.mb_skipped"),
        };
        if self.message.is_empty() {
            format!("{label}: {file} -> {}", self.name)
        } else {
            format!("{label}: {file} -> {}: {}", self.name, self.message)
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub total: usize,
    pub ok: usize,
    pub warnings: usize,
    pub errors: usize,
    pub skipped: usize,
    pub cancelled: usize,
    pub millis: u128,
}

impl Summary {
    pub fn of(results: &[FileResult], total: usize, millis: u128) -> Self {
        let count = |s: Status| results.iter().filter(|r| r.status == s).count();
        Self {
            total,
            ok: count(Status::Ok),
            warnings: count(Status::Warning),
            errors: count(Status::Error),
            skipped: count(Status::Skipped),
            cancelled: total.saturating_sub(results.len()),
            millis,
        }
    }

    pub fn converted(&self) -> usize {
        self.ok + self.warnings
    }

    pub fn text(&self) -> String {
        tf!(
            "entities.mb_summary",
            self.total,
            self.ok,
            self.warnings,
            self.errors,
            self.skipped,
            self.cancelled,
            self.millis
        )
    }
}

pub fn run(
    files: &[PathBuf],
    names: &[String],
    opts: &BatchOptions,
    root: &Path,
    cancel: &AtomicBool,
    progress: &(dyn Fn(&FileResult) + Sync),
) -> (Vec<FileResult>, Summary) {
    let started = Instant::now();
    let results: std::sync::Mutex<Vec<FileResult>> = std::sync::Mutex::new(Vec::new());
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..clamp_threads(opts.threads).min(files.len().max(1)) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= files.len() || cancel.load(Ordering::SeqCst) {
                        return;
                    }
                    let name = names.get(i).cloned().unwrap_or_else(|| clean_name("model"));
                    let result = convert_one(i, &files[i], &name, opts, root);
                    progress(&result);
                    if let Ok(mut list) = results.lock() {
                        list.push(result);
                    }
                }
            });
        }
    });
    let mut list = results.into_inner().unwrap_or_default();
    list.sort_by_key(|r| r.index);
    let summary = Summary::of(&list, files.len(), started.elapsed().as_millis());
    (list, summary)
}

fn convert_one(index: usize, path: &Path, name: &str, opts: &BatchOptions, root: &Path) -> FileResult {
    let base = FileResult {
        index,
        path: path.to_path_buf(),
        name: name.to_owned(),
        status: Status::Ok,
        message: String::new(),
        warnings: Vec::new(),
        converted: None,
    };
    if !opts.overwrite && model_exists(root, name) {
        return FileResult { status: Status::Skipped, message: t!("entities.mb_exists").to_owned(), ..base };
    }
    let outcome =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| convert_file(path, name, &opts.convert, root)))
            .unwrap_or_else(|_| Err(t!("entities.mb_crashed").to_owned()));
    match outcome {
        Ok(c) => {
            let status = if c.warnings.is_empty() { Status::Ok } else { Status::Warning };
            let message = c.warnings.join("; ");
            FileResult { status, message, warnings: c.warnings.clone(), converted: Some(c), ..base }
        }
        Err(e) => FileResult { status: Status::Error, message: e, ..base },
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridOptions {
    pub columns: usize,
    pub gap: f64,
}

impl Default for GridOptions {
    fn default() -> Self {
        Self { columns: 5, gap: 64.0 }
    }
}

pub fn grid_offsets(bounds: &[(DVec3, DVec3)], grid: GridOptions) -> Vec<DVec3> {
    let columns = grid.columns.clamp(1, 256);
    let gap = if grid.gap.is_finite() { grid.gap.clamp(0.0, 16384.0) } else { 64.0 };
    let (mut cell_x, mut cell_y) = (1.0f64, 1.0f64);
    for (lo, hi) in bounds {
        cell_x = cell_x.max(hi.x - lo.x);
        cell_y = cell_y.max(hi.y - lo.y);
    }
    bounds
        .iter()
        .enumerate()
        .map(|(i, (lo, hi))| {
            let (col, row) = ((i % columns) as f64, (i / columns) as f64);
            let center = DVec3::new((lo.x + hi.x) * 0.5, (lo.y + hi.y) * 0.5, 0.0);
            DVec3::new(col * (cell_x + gap), row * (cell_y + gap), -lo.z) - center
        })
        .collect()
}

pub const TR: TrTable = &[
    ("entities.mb_title", "Import models (batch)", "Importer des modèles (lot)"),
    ("entities.mb_pick_files", "Add files...", "Ajouter des fichiers..."),
    ("entities.mb_pick_folder", "Add a folder...", "Ajouter un dossier..."),
    ("entities.mb_clear", "Clear", "Vider"),
    ("entities.mb_recursive", "Include subfolders", "Inclure les sous-dossiers"),
    ("entities.mb_formats", "Formats", "Formats"),
    ("entities.mb_inputs", "{0} model files found", "{0} fichiers de modèle trouvés"),
    ("entities.mb_template", "Name template", "Modèle de nom"),
    (
        "entities.mb_template_tip",
        "{name} is the file name, {folder} the name of its folder, {index} its number. The prefix goes in front.",
        "{name} est le nom du fichier, {folder} le nom de son dossier, {index} son numéro. Le préfixe est placé devant.",
    ),
    ("entities.mb_prefix", "Prefix", "Préfixe"),
    ("entities.mb_overwrite", "Replace models that exist", "Remplacer les modèles qui existent"),
    (
        "entities.mb_overwrite_tip",
        "Off: a model whose name exists in models/hammerpro is skipped.",
        "Désactivé : un modèle dont le nom existe dans models/hammerpro est ignoré.",
    ),
    ("entities.mb_threads", "Parallel conversions", "Conversions en parallèle"),
    ("entities.mb_place", "Place one prop per model in a grid", "Placer un prop par modèle dans une grille"),
    ("entities.mb_columns", "Columns", "Colonnes"),
    ("entities.mb_gap", "Gap", "Écart"),
    ("entities.mb_scan", "Preview", "Aperçu"),
    ("entities.mb_scanning", "Reading {0} of {1} files...", "Lecture de {0} fichiers sur {1}..."),
    ("entities.mb_col_file", "File", "Fichier"),
    ("entities.mb_col_tris", "Triangles", "Triangles"),
    ("entities.mb_col_size", "Size", "Taille"),
    ("entities.mb_col_name", "Name", "Nom"),
    ("entities.mb_col_warnings", "Warnings", "Avertissements"),
    ("entities.mb_col_result", "Result", "Résultat"),
    ("entities.mb_start", "Convert {0} models", "Convertir {0} modèles"),
    ("entities.mb_cancel", "Cancel", "Annuler"),
    ("entities.mb_close", "Close", "Fermer"),
    ("entities.mb_running", "Converting {0} of {1}...", "Conversion de {0} sur {1}..."),
    ("entities.mb_no_files", "No model file found.", "Aucun fichier de modèle trouvé."),
    ("entities.mb_ok", "OK", "OK"),
    ("entities.mb_warning", "Warning", "Avertissement"),
    ("entities.mb_error", "Error", "Erreur"),
    ("entities.mb_skipped", "Skipped", "Ignoré"),
    ("entities.mb_exists", "the model exists already", "le modèle existe déjà"),
    ("entities.mb_crashed", "the conversion stopped unexpectedly", "la conversion s'est arrêtée de façon inattendue"),
    (
        "entities.mb_summary",
        "{0} files: {1} ok, {2} with warnings, {3} errors, {4} skipped, {5} not run ({6} ms)",
        "{0} fichiers : {1} ok, {2} avec avertissements, {3} erreurs, {4} ignorés, {5} non traités ({6} ms)",
    ),
    ("entities.mb_placed", "{0} props placed in a grid", "{0} props placés dans une grille"),
    (
        "entities.mb_where",
        "Models go in models/hammerpro/ of the game content, textures in materials/models/hammerpro/.",
        "Les modèles vont dans models/hammerpro/ du contenu du jeu, les textures dans materials/models/hammerpro/.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hp_batch_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    fn obj(path: &Path, size: f64) {
        let text = format!("v 0 0 0\nv {size} 0 0\nv 0 {size} 0\nv 0 0 {size}\nf 1 2 3\nf 1 2 4\nf 1 3 4\nf 2 3 4\n");
        std::fs::write(path, text).expect("obj");
    }

    #[test]
    fn names_follow_the_template_and_stay_unique() {
        crate::register_translations();
        let a = Path::new("C:/models/Chairs/Big Chair.obj");
        assert_eq!(apply_template("{name}", "", a, 1), "big_chair");
        assert_eq!(apply_template("{folder}_{name}", "", a, 1), "chairs_big_chair");
        assert_eq!(apply_template("{name}", "my_", a, 1), "my_big_chair");
        assert_eq!(apply_template("", "", a, 1), "big_chair");
        assert_eq!(apply_template("{name}_{index}", "", a, 7), "big_chair_007");
        assert_eq!(apply_template("!!!", "", a, 1), "model");
        let names =
            names_for(&[PathBuf::from("a/x.obj"), PathBuf::from("b/x.fbx"), PathBuf::from("c/x.glb")], "{name}", "");
        assert_eq!(names, ["x", "x_2", "x_3"]);
        let long = "a".repeat(60);
        let u = unique_names(vec![clean_name(&long), clean_name(&long)]);
        assert_eq!(u[0].len(), 48);
        assert_eq!(u[1].len(), 48);
        assert_ne!(u[0], u[1]);
    }

    #[test]
    fn files_are_filtered_by_format_and_recursion() {
        let dir = scratch("collect");
        std::fs::create_dir_all(dir.join("sub/deeper")).unwrap();
        for f in ["a.obj", "b.FBX", "c.glb", "d.gltf", "e.txt", "sub/f.obj", "sub/deeper/g.obj"] {
            std::fs::write(dir.join(f), "x").unwrap();
        }
        let names = |files: Vec<PathBuf>| -> Vec<String> {
            files.iter().map(|p| p.file_name().unwrap().to_string_lossy().to_ascii_lowercase()).collect()
        };
        let flat = names(collect_files(std::slice::from_ref(&dir), false, Formats::default()));
        assert_eq!(flat, ["a.obj", "b.fbx", "c.glb", "d.gltf"]);
        let deep = names(collect_files(std::slice::from_ref(&dir), true, Formats::default()));
        assert_eq!(deep.len(), 6);
        let only_obj = Formats { obj: true, fbx: false, gltf: false };
        assert_eq!(names(collect_files(std::slice::from_ref(&dir), true, only_obj)).len(), 3);
        let one = collect_files(&[dir.join("a.obj"), dir.join("a.obj"), dir.join("e.txt")], false, Formats::default());
        assert_eq!(one.len(), 1);
        assert!(Formats::parse("obj, glb").unwrap().gltf);
        assert!(Formats::parse("stl").is_err());
        assert!(Formats::parse("").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_small_batch_of_obj_files_converts_skips_and_reports() {
        crate::register_translations();
        let dir = scratch("run");
        let out = dir.join("out");
        std::fs::create_dir_all(dir.join("in")).unwrap();
        obj(&dir.join("in/box_a.obj"), 1.0);
        image::save_buffer(dir.join("in/red.png"), &[255u8, 0, 0, 255].repeat(16), 4, 4, image::ColorType::Rgba8)
            .unwrap();
        std::fs::write(dir.join("in/box_a.mtl"), "newmtl red\nmap_Kd red.png\n").unwrap();
        let text = std::fs::read_to_string(dir.join("in/box_a.obj")).unwrap();
        std::fs::write(dir.join("in/box_a.obj"), format!("mtllib box_a.mtl\nusemtl red\n{text}")).unwrap();
        obj(&dir.join("in/box_b.obj"), 0.001);
        std::fs::write(dir.join("in/broken.obj"), "just text\n").unwrap();
        let files = collect_files(&[dir.join("in")], false, Formats::default());
        assert_eq!(files.len(), 3);
        let opts = BatchOptions { threads: 2, prefix: "t_".to_owned(), ..BatchOptions::default() };
        let names = names_for(&files, &opts.template, &opts.prefix);
        let entries = scan(&files, &opts, Some(&out), &AtomicBool::new(false), &AtomicUsize::new(0));
        assert_eq!(entries.len(), 3);
        let a = entries.iter().find(|e| e.name == "t_box_a").expect("box_a");
        assert_eq!(a.triangles, 4);
        assert!(a.error.is_none());
        let b = entries.iter().find(|e| e.name == "t_box_b").expect("box_b");
        assert!(!b.warnings.is_empty(), "tiny model warns: {:?}", b.warnings);
        assert!(entries.iter().find(|e| e.name == "t_broken").expect("broken").error.is_some());

        let seen = AtomicUsize::new(0);
        let (results, summary) = run(&files, &names, &opts, &out, &AtomicBool::new(false), &|_| {
            seen.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(seen.load(Ordering::SeqCst), 3);
        assert_eq!(summary.total, 3);
        assert_eq!((summary.ok, summary.warnings, summary.errors, summary.skipped), (1, 1, 1, 0), "{results:?}");
        assert!(out.join("models/hammerpro/t_box_a.mdl").is_file());
        assert!(out.join("models/hammerpro/t_box_b.mdl").is_file());
        assert!(!out.join("models/hammerpro/t_broken.mdl").exists());
        assert_eq!(results[0].model_path(), Some("models/hammerpro/t_box_a.mdl"));
        assert!(results[0].line().contains("t_box_a"));

        let (again, summary) = run(&files, &names, &opts, &out, &AtomicBool::new(false), &|_| {});
        assert_eq!(summary.skipped, 2, "{again:?}");
        assert_eq!(summary.errors, 1);
        let replace = BatchOptions { overwrite: true, ..opts.clone() };
        let (_, summary) = run(&files, &names, &replace, &out, &AtomicBool::new(false), &|_| {});
        assert_eq!((summary.skipped, summary.converted()), (0, 2));

        let (partial, summary) = run(&files, &names, &opts, &out, &AtomicBool::new(true), &|_| {});
        assert!(partial.is_empty());
        assert_eq!(summary.cancelled, 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn grid_offsets_rest_on_the_floor_in_rows() {
        let b = |w: f64| (DVec3::new(-w, -w, 0.0), DVec3::new(w, w, 10.0));
        let offsets = grid_offsets(&[b(10.0), b(20.0), b(5.0)], GridOptions { columns: 2, gap: 10.0 });
        assert_eq!(offsets.len(), 3);
        assert_eq!(offsets[0], DVec3::new(0.0, 0.0, 0.0));
        assert_eq!(offsets[1], DVec3::new(50.0, 0.0, 0.0));
        assert_eq!(offsets[2], DVec3::new(0.0, 50.0, 0.0));
        let raised = grid_offsets(&[(DVec3::new(0.0, 0.0, 5.0), DVec3::new(2.0, 2.0, 9.0))], GridOptions::default());
        assert_eq!(raised[0], DVec3::new(-1.0, -1.0, -5.0));
    }
}
