use std::collections::{BTreeSet, HashSet};
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::time::Instant;

use glam::DVec3;
use hp_app::i18n::Lang;
use hp_app::sidecar::Sidecar;
use hp_app::view::Camera3d;
use hp_app::{AppState, Severity, t, tf};
use hp_editor::{Document, EditError, EditResult, ObjRef};
use hp_geom::Aabb;
use hp_vmf::{Id, LoadReport};

pub use crate::opening::OpenKind;

#[derive(Clone, Debug, PartialEq)]
pub enum PendingOp {
    New,
    Open(PathBuf),
    Recover { autosave: PathBuf, original: Option<PathBuf> },
    Exit,
    Close,
    NewTab,
    OpenTab(PathBuf),
}

impl PendingOp {
    pub fn unsaved_question(&self, map_title: &str) -> String {
        match self {
            PendingOp::New | PendingOp::NewTab => tf!("dialog.unsaved_body_new", map_title),
            PendingOp::Open(path) | PendingOp::OpenTab(path) | PendingOp::Recover { autosave: path, .. } => {
                tf!("dialog.unsaved_body_open", map_title, file_name(path))
            }
            PendingOp::Exit => tf!("dialog.unsaved_body", map_title),
            PendingOp::Close => tf!("tabs.close_question", map_title),
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn is_damaged(doc: &Document) -> bool {
    doc.load_report().is_some_and(LoadReport::is_damaged)
}

pub fn map_display_name(state: &AppState) -> String {
    doc_display_name(&state.doc)
}

pub fn doc_display_name(doc: &Document) -> String {
    if doc.path().is_none()
        && is_damaged(doc)
        && let Some(from) = doc.opened_from()
    {
        return tf!("app.title_damaged", file_name(from));
    }
    if let Some(name) = doc.manifest().and_then(|m| m.path.file_name()) {
        return name.to_string_lossy().into_owned();
    }
    if let Some(name) = doc.path().and_then(Path::file_name) {
        return name.to_string_lossy().into_owned();
    }
    match doc.opened_from().and_then(Path::file_name) {
        Some(name) => tf!("app.damaged_title", name.to_string_lossy()),
        None => t!("app.untitled").to_owned(),
    }
}

fn start_dir(state: &AppState) -> Option<PathBuf> {
    state
        .doc
        .path()
        .or_else(|| state.doc.opened_from())
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .or_else(|| state.settings.game.map_dir.clone())
        .or_else(|| state.settings.recent_files.first().and_then(|p| p.parent().map(Path::to_path_buf)))
}

pub fn pick_open_vmf(state: &AppState) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new()
        .add_filter(t!("dialog.vmf_filter"), &["vmf", "vmx", "vmm"])
        .add_filter(t!("app.map_filter"), &["map"]);
    if let Some(dir) = start_dir(state) {
        dialog = dialog.set_directory(dir);
    }
    dialog.pick_file()
}

pub fn pick_save_obj(state: &AppState, default_name: &str) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().add_filter(t!("startup.obj_filter"), &["obj"]).set_file_name(default_name);
    if let Some(dir) = start_dir(state) {
        dialog = dialog.set_directory(dir);
    }
    dialog.save_file().map(|p| if p.extension().is_none() { p.with_extension("obj") } else { p })
}

pub fn pick_save_vmf(state: &AppState, default_name: &str) -> Option<PathBuf> {
    let mut dialog = rfd::FileDialog::new().add_filter(t!("dialog.vmf_filter"), &["vmf"]).set_file_name(default_name);
    if let Some(dir) = start_dir(state) {
        dialog = dialog.set_directory(dir);
    }
    dialog.save_file().map(|p| if p.extension().is_none() { p.with_extension("vmf") } else { p })
}

const SPAWN_CLASSES: &[&str] =
    &["info_player_start", "info_player_deathmatch", "info_player_terrorist", "info_player_counterterrorist"];

const EYE_HEIGHT: f64 = 64.0;
const CAMERA_LOOK_DISTANCE: f64 = 512.0;

fn saved_camera(state: &AppState) -> Option<(DVec3, DVec3)> {
    let cams = state.doc.scene().cameras();
    let index = usize::try_from(cams.active).ok()?;
    let cam = cams.cameras.get(index)?;
    let valid = cam.position.is_finite() && cam.look.is_finite();
    (valid && (cam.look - cam.position).length_squared() > 1e-6).then_some((cam.position, cam.look))
}

fn clear_distance(state: &AppState, from: DVec3, dir: DVec3, distance: f64) -> f64 {
    let opts = hp_editor::PickOptions { include_entities: false, max_distance: distance, ..Default::default() };
    let ray = hp_geom::Ray::new(from, dir);
    match state.doc.pick_ray(&ray, &opts, &|e| state.entity_bounds(e)) {
        Some(hit) => (hit.t - 8.0).max(0.0),
        None => distance,
    }
}

fn spawn_camera(state: &AppState) -> Option<(DVec3, f64)> {
    let entities = state.doc.scene().entities();
    let entity = SPAWN_CLASSES.iter().find_map(|class| {
        entities.values().map(|n| &n.entity).find(|e| e.classname.eq_ignore_ascii_case(class) && e.origin().is_some())
    })?;
    let origin = entity.origin().filter(|o| o.is_finite())?;
    let yaw = entity.angles().map(|a| a.y).filter(|y| y.is_finite()).unwrap_or(0.0);
    let (sin, cos) = yaw.to_radians().sin_cos();
    let forward = DVec3::new(cos, sin, 0.0);
    let (reach, top) =
        match state.entity_bounds(entity).filter(|b| !b.is_empty() && b.min.is_finite() && b.max.is_finite()) {
            Some(b) => {
                let r = (b.min - origin).abs().max((b.max - origin).abs());
                (r.x.max(r.y).min(256.0) * std::f64::consts::SQRT_2, (b.max.z - origin.z).clamp(0.0, 256.0))
            }
            None => (16.0 * std::f64::consts::SQRT_2, 72.0),
        };
    let eye = origin + DVec3::new(0.0, 0.0, EYE_HEIGHT);
    let wanted = reach + 8.0;
    if clear_distance(state, eye, forward, wanted + 8.0) >= wanted {
        Some((eye + forward * wanted, yaw))
    } else {
        Some((origin + DVec3::new(0.0, 0.0, top.max(EYE_HEIGHT) + 16.0), yaw))
    }
}

pub(crate) fn drop_in_background(doc: Document) {
    if doc.scene().solids().len() < 1000 {
        return;
    }
    if let Err(e) = std::thread::Builder::new().name("hp-drop-map".to_owned()).spawn(move || drop(doc)) {
        log::debug!("dropping the previous map on the UI thread: {e}");
    }
}

fn solid_bounds(doc: &Document) -> Aabb {
    let finite = |b: &Aabb| !b.is_empty() && b.min.is_finite() && b.max.is_finite();
    let (mut valid, mut invalid) = (Aabb::EMPTY, Aabb::EMPTY);
    for n in doc.scene().solids().values().filter(|n| finite(&n.bounds)) {
        if n.geom.valid {
            valid = valid.union(&n.bounds);
        } else {
            invalid = invalid.union(&n.bounds);
        }
    }
    if valid.is_empty() { invalid } else { valid }
}

pub fn map_bounds(state: &AppState) -> Option<Aabb> {
    let finite = |b: &Aabb| !b.is_empty() && b.min.is_finite() && b.max.is_finite();
    let mut bounds = Some(solid_bounds(&state.doc)).filter(finite).unwrap_or(Aabb::EMPTY);
    let scene = state.doc.scene();
    for (id, node) in scene.entities() {
        if scene.is_brush_entity(*id) {
            continue;
        }
        let Some(origin) = node.entity.origin().filter(|o| o.is_finite()) else { continue };
        let b = state
            .entity_bounds(&node.entity)
            .filter(finite)
            .unwrap_or_else(|| Aabb::new(origin - DVec3::splat(8.0), origin + DVec3::splat(8.0)));
        bounds = bounds.union(&b);
    }
    finite(&bounds).then_some(bounds)
}

pub fn frame_map(state: &mut AppState, view_size: egui::Vec2) {
    match map_bounds(state) {
        Some(b) => {
            state.cameras.frame_2d(b.min, b.max, view_size * 0.5);
            let mut cam = state.cameras.cam3d;
            if let Some((pos, look)) = saved_camera(state) {
                cam.pos = pos;
                cam.look_at(look);
            } else if let Some((eye, yaw)) = spawn_camera(state) {
                cam.pos = eye;
                cam.yaw_deg = yaw;
                cam.pitch_deg = -10.0;
            } else {
                cam.yaw_deg = 45.0;
                cam.pitch_deg = -30.0;
                cam.frame_box(b.min, b.max);
            }
            state.cameras.cam3d = cam;
        }
        None => {
            state.cameras = hp_app::state::Cameras::default();
        }
    }
}

fn cameras_with_view(existing: &hp_vmf::Cameras, cam: &Camera3d) -> hp_vmf::Cameras {
    let mut out = existing.clone();
    if !cam.pos.is_finite() || !cam.forward().is_finite() {
        return out;
    }
    let tidy = |v: DVec3| (v * 1000.0).round() / 1000.0;
    let current =
        hp_vmf::Camera { position: tidy(cam.pos), look: tidy(cam.pos + cam.forward() * CAMERA_LOOK_DISTANCE) };
    match usize::try_from(out.active).ok().filter(|&i| i < out.cameras.len()) {
        Some(i) => out.cameras[i] = current,
        None => {
            out.cameras.push(current);
            out.active = i32::try_from(out.cameras.len() - 1).unwrap_or(0);
        }
    }
    out
}

fn view_settings_values(settings: &hp_app::settings::Settings) -> [(&'static str, String); 3] {
    let edit = &settings.edit;
    let spacing = edit.grid_size.round().clamp(1.0, 1024.0) as i64;
    let flag = |b: bool| if b { "1" } else { "0" }.to_owned();
    [
        ("nGridSpacing", spacing.to_string()),
        ("bSnapToGrid", flag(edit.snap_to_grid)),
        ("bShowGrid", flag(edit.show_grid)),
    ]
}

pub fn store_view(state: &AppState, map: &mut hp_vmf::Map) {
    store_view_with(&state.cameras, &state.settings, map);
}

pub fn store_view_with(cameras: &hp_app::state::Cameras, settings: &hp_app::settings::Settings, map: &mut hp_vmf::Map) {
    map.cameras = cameras_with_view(&map.cameras, &cameras.cam3d);
    for (key, value) in view_settings_values(settings) {
        map.view_settings.set(key, value);
    }
}

pub fn store_view_in_doc(state: &mut AppState) {
    let cameras = cameras_with_view(state.doc.scene().cameras(), &state.cameras.cam3d);
    if &cameras != state.doc.scene().cameras() {
        state.doc.set_cameras(cameras);
    }
    for (key, value) in view_settings_values(&state.settings) {
        if state.doc.scene().view_settings().get(key) != Some(value.as_str()) {
            state.doc.set_view_setting(key, &value);
        }
    }
}

fn apply_saved_grid(state: &mut AppState) {
    let saved = state.doc.scene().view_settings().get("nGridSpacing").and_then(|v| v.trim().parse::<f64>().ok());
    if let Some(g) = saved.filter(|g| g.is_finite() && (1.0..=1024.0).contains(g)) {
        state.settings.edit.grid_size = g;
    }
}

#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ViewState {
    pub cam3d: Option<([f64; 3], f64, f64)>,
    pub top: Option<hp_app::view::Camera2d>,
    pub front: Option<hp_app::view::Camera2d>,
    pub side: Option<hp_app::view::Camera2d>,
    pub tool: Option<String>,
}

impl hp_app::sidecar::SidecarSection for ViewState {
    const NAME: &'static str = "startup_view";
}

fn valid_2d(c: &hp_app::view::Camera2d) -> bool {
    c.center.is_finite() && c.zoom.is_finite() && (1.0e-6..=1.0e4).contains(&c.zoom)
}

impl ViewState {
    pub fn capture(state: &AppState) -> Self {
        let c = &state.cameras;
        Self {
            cam3d: Some((c.cam3d.pos.to_array(), c.cam3d.yaw_deg, c.cam3d.pitch_deg)),
            top: Some(c.top),
            front: Some(c.front),
            side: Some(c.side),
            tool: Some(format!("{:?}", state.tool)),
        }
    }

    pub fn apply_cameras(&self, state: &mut AppState) {
        if let Some((pos, yaw, pitch)) = self.cam3d {
            let pos = DVec3::from_array(pos);
            if pos.is_finite() && yaw.is_finite() && pitch.is_finite() {
                let cam = &mut state.cameras.cam3d;
                cam.pos = pos;
                cam.yaw_deg = yaw;
                cam.pitch_deg = pitch.clamp(-89.0, 89.0);
            }
        }
        for (saved, cam) in [
            (self.top, &mut state.cameras.top),
            (self.front, &mut state.cameras.front),
            (self.side, &mut state.cameras.side),
        ] {
            if let Some(saved) = saved.filter(valid_2d) {
                *cam = saved;
            }
        }
    }

    pub fn tool(&self) -> Option<hp_app::ToolId> {
        let name = self.tool.as_deref()?;
        hp_app::ToolId::ALL.into_iter().find(|t| format!("{t:?}") == name)
    }
}

pub fn remember_view(state: &mut AppState) {
    let view = ViewState::capture(state);
    state.sidecar.set(&view);
}

pub fn flush_sidecar(state: &mut AppState) {
    remember_view(state);
    if let Err(e) = state.sidecar.flush() {
        state.log(Severity::Warning, tf!("app.sidecar_failed", e));
    }
}

fn switch_sidecar(state: &mut AppState) {
    flush_sidecar(state);
    let dir = state.sidecar.dir().map(Path::to_path_buf);
    state.sidecar = Sidecar::open(dir.as_deref(), state.doc.path());
    hp_app::games::apply_map_game(state);
}

pub fn new_map(state: &mut AppState, view_size: egui::Vec2) {
    state.doc = Document::new_empty();
    switch_sidecar(state);
    apply_doc_options(state);
    state.doc.mark_everything_changed();
    state.pointfile = None;
    frame_map(state, view_size);
}

pub fn apply_doc_options(state: &mut AppState) {
    let lock = state.settings.edit.texture_lock;
    let scale_lock = state.settings.edit.texture_scale_lock;
    let opts = state.doc.options_mut();
    opts.texture_lock = lock;
    opts.texture_scale_lock = scale_lock;
    let limits = hp_editor::UndoLimits {
        max_steps: state.settings.edit.undo_levels.max(1) as usize,
        ..hp_editor::UndoLimits::default()
    };
    state.doc.set_undo_limits(limits);
}

const DAMAGED_NOTE_EN: &str =
    "The map was opened as a new document so that saving does not overwrite the damaged file: use Save As";

fn tr_args(key: &str, lang: Lang, args: &[&dyn Display]) -> String {
    hp_app::i18n::format_placeholders(hp_app::i18n::tr_in(key, lang), args)
}

fn tr_count(n: usize, one: &str, many: &str, lang: Lang) -> String {
    if n == 1 { tr_args(one, lang, &[]) } else { tr_args(many, lang, &[&n]) }
}

fn report_lines(report: &LoadReport, lang: Lang) -> Vec<String> {
    let s = &report.syntax;
    let mut out = Vec::new();
    if s.unterminated_string {
        out.push(tr_args("app.load.unterminated", lang, &[]));
    }
    if s.unclosed_blocks > 0 {
        out.push(tr_count(s.unclosed_blocks, "app.load.unclosed_one", "app.load.unclosed_many", lang));
    }
    if s.stray_closes > 0 {
        out.push(tr_count(s.stray_closes, "app.load.stray_one", "app.load.stray_many", lang));
    }
    if s.missing_values > 0 {
        out.push(tr_count(s.missing_values, "app.load.no_value_one", "app.load.no_value_many", lang));
    }
    if report.unreadable_values > 0 {
        let text = tr_count(report.unreadable_values, "app.load.unreadable_one", "app.load.unreadable_many", lang);
        if report.unreadable_examples.is_empty() {
            out.push(text);
        } else {
            out.push(tr_args("app.load.examples", lang, &[&text, &report.unreadable_examples.join("; ")]));
        }
    }
    if report.missing_ids > 0 {
        out.push(tr_count(report.missing_ids, "app.load.no_id_one", "app.load.no_id_many", lang));
    }
    out
}

pub fn load_warning_lines(doc: &Document, lang: Lang) -> Vec<String> {
    let warnings = doc.load_warnings();
    let Some(report) = doc.load_report() else { return warnings.to_vec() };
    let english = report.warnings();
    let translated = report_lines(report, lang);
    let mut out = if translated.len() == english.len() { translated } else { english.clone() };
    for w in warnings.iter().filter(|w| !english.contains(w)) {
        if w == DAMAGED_NOTE_EN {
            out.push(tr_args("app.load.damaged_note", lang, &[]));
        } else {
            out.push(w.clone());
        }
    }
    out
}

fn announce_load(state: &mut AppState, name: &str, opened: String) {
    let lines = load_warning_lines(&state.doc, hp_app::i18n::lang());
    let damaged = is_damaged(&state.doc);
    if lines.is_empty() && !damaged {
        state.notify(Severity::Success, opened);
        return;
    }
    state.log(Severity::Info, opened);
    for line in &lines {
        state.log(Severity::Warning, line.clone());
    }
    let toast = if damaged {
        tf!("app.load_damaged", name)
    } else if lines.len() == 1 {
        tf!("app.load_problems_one", name)
    } else {
        tf!("app.load_problems_many", name, lines.len())
    };
    state.toasts.push(Severity::Warning, toast);
}

pub fn announce_import(state: &mut AppState, report: &hp_vmf::import_map::MapImportReport) {
    state.log(Severity::Info, tf!("app.map_imported", report.brushes, report.entities));
    let skipped = report.skipped_blocks + report.bad_faces;
    if skipped > 0 {
        state.log(Severity::Warning, tf!("app.map_skipped", skipped));
    }
    state.toasts.push(Severity::Info, t!("app.map_save_as"));
}

fn install_document(state: &mut AppState, doc: Document, recent: Option<&Path>, view_size: egui::Vec2) {
    let old = std::mem::replace(&mut state.doc, doc);
    drop_in_background(old);
    switch_sidecar(state);
    apply_doc_options(state);
    state.doc.mark_everything_changed();
    state.pointfile = None;
    if let Some(path) = recent {
        state.settings.add_recent(path);
        state.settings_dirty = true;
    }
    apply_saved_grid(state);
    frame_map(state, view_size);
    state.sidecar.get::<ViewState>().apply_cameras(state);
}

pub fn install_loaded(
    state: &mut AppState,
    path: &Path,
    kind: &OpenKind,
    result: EditResult<Document>,
    started: Instant,
    view_size: egui::Vec2,
) -> bool {
    let mut doc = match result {
        Ok(doc) => doc,
        Err(e) => {
            state.notify(Severity::Error, tf!("dialog.open_failed", crate::dispatch::edit_error_text(&e)));
            return false;
        }
    };
    let name = file_name(path);
    match kind {
        OpenKind::Map => {
            install_document(state, doc, Some(path), view_size);
            let opened = tf!("status.opened", name, started.elapsed().as_millis());
            announce_load(state, &name, opened);
        }
        OpenKind::Recovered { original } => {
            if !is_damaged(&doc) {
                doc.set_path(original.clone());
            }
            doc.mark_dirty();
            install_document(state, doc, original.as_deref(), view_size);
            announce_load(state, &name, tf!("app.recovered", name));
        }
    }
    true
}

#[cfg(test)]
pub fn open_map(state: &mut AppState, path: &Path, view_size: egui::Vec2) -> bool {
    let started = Instant::now();
    install_loaded(state, path, &OpenKind::Map, Document::open(path), started, view_size)
}

#[cfg(test)]
pub fn open_recovered(state: &mut AppState, autosave: &Path, original: Option<&Path>, view_size: egui::Vec2) -> bool {
    let kind = OpenKind::Recovered { original: original.map(Path::to_path_buf) };
    install_loaded(state, autosave, &kind, Document::open(autosave), Instant::now(), view_size)
}

pub fn save_map_to(state: &mut AppState, path: &Path) -> bool {
    store_view_in_doc(state);
    remember_view(state);
    let seq = crate::autosave::next_save_seq();
    let saved = crate::autosave::write_in_order(path, seq, || state.doc.save(path)).unwrap_or(Ok(()));
    match saved {
        Ok(()) => {
            if let Err(e) = state.sidecar.save_as(path) {
                state.log(Severity::Warning, tf!("app.sidecar_failed", e));
            }
            state.settings.add_recent(path);
            state.settings_dirty = true;
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            state.notify(Severity::Success, tf!("status.saved", name));
            true
        }
        Err(e) => {
            state.notify(Severity::Error, tf!("dialog.save_failed", crate::dispatch::edit_error_text(&e)));
            false
        }
    }
}

pub fn save_map(state: &mut AppState) -> bool {
    match state.doc.path().map(Path::to_path_buf) {
        Some(path) => save_map_to(state, &path),
        None => save_map_as(state),
    }
}

pub fn default_save_name(doc: &Document) -> String {
    if let Some(name) = doc.path().and_then(|p| p.file_name()) {
        return name.to_string_lossy().into_owned();
    }
    let Some(from) = doc.opened_from() else { return "untitled.vmf".to_owned() };
    let stem = from.file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty());
    let stem = stem.unwrap_or_else(|| "untitled".to_owned());
    let dir = from.parent().unwrap_or(Path::new(""));
    (1..1000)
        .map(|n| if n == 1 { format!("{stem}_recovered.vmf") } else { format!("{stem}_recovered_{n}.vmf") })
        .find(|name| !dir.join(name).exists())
        .unwrap_or_else(|| format!("{stem}_recovered.vmf"))
}

pub fn save_map_as(state: &mut AppState) -> bool {
    let default_name = default_save_name(&state.doc);
    match pick_save_vmf(state, &default_name) {
        Some(path) => save_map_to(state, &path),
        None => false,
    }
}

pub fn import_map(state: &mut AppState) {
    let Some(path) = pick_open_vmf(state) else { return };
    import_map_from(state, &path);
}

pub fn import_map_from(state: &mut AppState, path: &Path) -> Option<usize> {
    let name = file_name(path);
    match state.doc.import_file(path) {
        Ok(objs) => {
            let n = objs.len();
            let text = if n == 1 { tf!("app.imported_one", name) } else { tf!("app.imported_many", n, name) };
            state.notify(Severity::Success, text);
            Some(n)
        }
        Err(EditError::Nothing(_)) => {
            state.notify(Severity::Info, tf!("app.import_empty", name));
            None
        }
        Err(e) => {
            state.notify(Severity::Error, tf!("app.import_failed", name, crate::dispatch::edit_error_text(&e)));
            None
        }
    }
}

fn visgroup_refs(e: &hp_vmf::EditorInfo, out: &mut HashSet<Id>) {
    out.extend(e.visgroup_ids.iter().copied());
}

fn filter_visgroups(tree: &[hp_vmf::VisGroup], used: &HashSet<Id>) -> Vec<hp_vmf::VisGroup> {
    tree.iter()
        .filter_map(|v| {
            let children = filter_visgroups(&v.children, used);
            (used.contains(&v.id) || !children.is_empty()).then(|| hp_vmf::VisGroup { children, ..v.clone() })
        })
        .collect()
}

fn collect_visgroup_ids(tree: &[hp_vmf::VisGroup], out: &mut HashSet<Id>) {
    for v in tree {
        out.insert(v.id);
        collect_visgroup_ids(&v.children, out);
    }
}

pub fn export_map(doc: &Document) -> Result<hp_vmf::Map, String> {
    let text = doc.copy_selection();
    let (mut solids, mut entities) = hp_vmf::parse_fragment(&text).map_err(|e| e.to_string())?;
    let scene = doc.scene();
    let exported: HashSet<ObjRef> =
        solids.iter().map(|s| ObjRef::Solid(s.id)).chain(entities.iter().map(|e| ObjRef::Entity(e.id))).collect();

    let mut groups: BTreeSet<Id> = solids
        .iter()
        .map(|s| &s.editor)
        .chain(entities.iter().map(|e| &e.editor))
        .filter_map(|e| e.group_id)
        .filter(|g| scene.group(*g).is_some())
        .collect();
    let mut stack: Vec<Id> = groups.iter().copied().collect();
    while let Some(g) = stack.pop() {
        if let Some(parent) = scene.group_parent(g)
            && !groups.contains(&parent)
        {
            let members = scene.group_members(parent);
            if !members.is_empty() && members.iter().all(|m| exported.contains(m)) {
                groups.insert(parent);
                stack.push(parent);
            }
        }
    }
    let mut world_groups: Vec<hp_vmf::Group> = groups
        .iter()
        .filter_map(|g| scene.group(*g))
        .map(|n| {
            let mut g = n.group.clone();
            if g.editor.group_id.is_some_and(|p| !groups.contains(&p)) {
                g.editor.group_id = None;
            }
            g
        })
        .collect();

    let mut used = HashSet::new();
    for s in solids.iter().chain(entities.iter().flat_map(|e| e.solids.iter())) {
        visgroup_refs(&s.editor, &mut used);
    }
    for e in &entities {
        visgroup_refs(&e.editor, &mut used);
    }
    for g in &world_groups {
        visgroup_refs(&g.editor, &mut used);
    }
    let visgroups = filter_visgroups(scene.visgroups(), &used);
    let mut known = HashSet::new();
    collect_visgroup_ids(&visgroups, &mut known);
    let fix = |e: &mut hp_vmf::EditorInfo| {
        e.visgroup_ids.retain(|v| known.contains(v));
        if e.group_id.is_some_and(|g| !groups.contains(&g)) {
            e.group_id = None;
        }
    };
    for s in &mut solids {
        fix(&mut s.editor);
    }
    for e in &mut entities {
        fix(&mut e.editor);
        for s in &mut e.solids {
            s.editor.visgroup_ids.retain(|v| known.contains(v));
        }
    }
    for g in &mut world_groups {
        g.editor.visgroup_ids.retain(|v| known.contains(v));
    }

    let mut map = hp_vmf::Map::new_empty();
    map.world.solids = solids;
    map.world.groups = world_groups;
    map.entities = entities;
    map.visgroups = visgroups;
    Ok(map)
}

pub fn export_selection(state: &mut AppState) {
    let map = match export_map(&state.doc) {
        Ok(map) => map,
        Err(e) => {
            state.notify(Severity::Error, tf!("dialog.save_failed", e));
            return;
        }
    };
    let Some(path) = pick_save_vmf(state, "prefab.vmf") else { return };
    match hp_vmf::save(&map, &path) {
        Ok(()) => state.notify(Severity::Success, tf!("status.saved", path.display())),
        Err(e) => state.notify(Severity::Error, tf!("dialog.save_failed", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state_with(vmf: &str) -> AppState {
        let mut state = hp_app::testing::test_state();
        state.doc = Document::from_map(hp_vmf::parse(vmf).expect("test map parses"));
        state
    }

    fn grouped_vmf() -> String {
        let vmf = hp_app::testing::TEST_VMF;
        let editor = "editor { \"color\" \"0 180 255\" \"visgroupshown\" \"1\" \"visgroupautoshown\" \"1\" }";
        let grouped = "editor { \"color\" \"0 180 255\" \"visgroupid\" \"7\" \"groupid\" \"50\" \"visgroupshown\" \"0\" \"visgroupautoshown\" \"1\" }";
        let mut out = vmf.replacen(editor, grouped, 2);
        out = out.replacen(
            "editor { \"color\" \"220 30 220\" \"visgroupshown\" \"1\" \"visgroupautoshown\" \"1\" }\n}\nentity\n{\n\t\"id\" \"6\"",
            "editor { \"color\" \"220 30 220\" \"visgroupid\" \"8\" \"groupid\" \"51\" \"visgroupshown\" \"1\" \"visgroupautoshown\" \"1\" }\n}\nentity\n{\n\t\"id\" \"6\"",
            1,
        );
        out = out.replacen(
            "\t\"skyname\" \"sky_day01_01\"",
            "\t\"skyname\" \"sky_day01_01\"\n\tgroup { \"id\" \"50\" editor { \"color\" \"0 100 0\" \"groupid\" \"51\" \"visgroupshown\" \"1\" \"visgroupautoshown\" \"1\" } }\n\tgroup { \"id\" \"51\" editor { \"color\" \"0 100 0\" \"visgroupshown\" \"1\" \"visgroupautoshown\" \"1\" } }",
            1,
        );
        let visgroups = "visgroups\n{\n\tvisgroup { \"name\" \"Floor\" \"visgroupid\" \"7\" \"color\" \"1 2 3\" }\n\tvisgroup { \"name\" \"Parent\" \"visgroupid\" \"9\" \"color\" \"1 2 3\"\n\t\tvisgroup { \"name\" \"Lights\" \"visgroupid\" \"8\" \"color\" \"1 2 3\" }\n\t}\n\tvisgroup { \"name\" \"Unused\" \"visgroupid\" \"10\" \"color\" \"1 2 3\" }\n}\nworld\n";
        out.replacen("world\n", visgroups, 1)
    }

    #[test]
    fn export_keeps_full_groups_and_used_visgroups() {
        let mut state = state_with(&grouped_vmf());
        assert_eq!(state.doc.scene().groups().len(), 2, "test map has two groups");
        state.doc.select(&[ObjRef::Solid(2), ObjRef::Solid(3)], hp_editor::SelectOp::Replace);
        let map = export_map(&state.doc).unwrap();
        let ids: Vec<Id> = map.world.groups.iter().map(|g| g.id).collect();
        assert_eq!(ids, vec![50]);
        assert_eq!(map.world.groups[0].editor.group_id, None, "parent group 51 is not exported");
        assert!(map.world.solids.iter().all(|s| s.editor.group_id == Some(50)));
        let names: Vec<&str> = map.visgroups.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["Floor"]);
        assert!(map.world.solids.iter().all(|s| s.editor.visgroup_ids == vec![7] && !s.editor.visgroup_shown));

        state.doc.select(&[ObjRef::Solid(2), ObjRef::Solid(3), ObjRef::Entity(5)], hp_editor::SelectOp::Replace);
        let map = export_map(&state.doc).unwrap();
        let ids: Vec<Id> = map.world.groups.iter().map(|g| g.id).collect();
        assert_eq!(ids, vec![50, 51]);
        assert_eq!(map.world.groups[0].editor.group_id, Some(51));
        let names: Vec<&str> = map.visgroups.iter().map(|v| v.name.as_str()).collect();
        assert_eq!(names, vec!["Floor", "Parent"]);
        assert_eq!(map.visgroups[1].children[0].name, "Lights");
        let back = hp_vmf::parse(&hp_vmf::write(&map)).unwrap();
        assert_eq!(back.world.groups.len(), 2);
        assert_eq!(back.visgroups.len(), 2);
    }

    #[test]
    fn spawn_camera_stands_outside_the_player_model() {
        let state = hp_app::testing::test_state();
        let (eye, yaw) = spawn_camera(&state).unwrap();
        assert_eq!(yaw, 90.0);
        let origin = DVec3::new(0.0, -128.0, 0.0);
        let model = Aabb::new(origin + DVec3::new(-16.0, -16.0, 0.0), origin + DVec3::new(16.0, 16.0, 72.0));
        assert!(!model.contains_point(eye), "eye {eye:?} is inside the model");
        assert!(eye.y > -128.0 + 16.0, "in front of the spawn: {eye:?}");
        assert!((eye.z - EYE_HEIGHT).abs() < 1e-9);
    }

    #[test]
    fn framing_includes_point_entities() {
        let mut state = hp_app::testing::test_state();
        let far = hp_app::testing::TEST_VMF.replace("\"origin\" \"0 -128 0\"", "\"origin\" \"-1024 0 0\"");
        state.doc = Document::from_map(hp_vmf::parse(&far).unwrap());
        let solids_only = state.doc.stats().bounds.unwrap();
        let all = map_bounds(&state).unwrap();
        assert!(solids_only.min.x > -1024.0, "{solids_only:?}");
        assert!(all.min.x <= -1024.0 - 8.0, "{all:?}");
        let only_entities = "versioninfo { \"editorversion\" \"400\" }\nworld { \"id\" \"1\" \"classname\" \"worldspawn\" }\nentity { \"id\" \"2\" \"classname\" \"light\" \"origin\" \"512 512 64\" }\n";
        state.doc = Document::from_map(hp_vmf::parse(only_entities).unwrap());
        let b = map_bounds(&state).unwrap();
        assert!(b.min.x < 512.0 && b.max.x > 512.0);
    }

    #[test]
    fn saving_stores_the_camera_and_grid() {
        let mut state = hp_app::testing::test_state();
        state.cameras.cam3d.pos = DVec3::new(100.0, 200.0, 300.0);
        state.cameras.cam3d.yaw_deg = 45.0;
        state.cameras.cam3d.pitch_deg = -10.0;
        state.settings.edit.grid_size = 4.0;
        let dirty = state.doc.is_dirty();
        store_view_in_doc(&mut state);
        assert_eq!(state.doc.is_dirty(), dirty, "storing the view does not modify the map");
        let cams = state.doc.scene().cameras().clone();
        assert_eq!(cams.active, 0);
        assert_eq!(cams.cameras[0].position, DVec3::new(100.0, 200.0, 300.0));
        assert_eq!(state.doc.scene().view_settings().get("nGridSpacing"), Some("4"));
        let text = state.doc.to_vmf_string();
        let mut other = hp_app::testing::test_state();
        other.doc = Document::from_map(hp_vmf::parse(&text).unwrap());
        apply_saved_grid(&mut other);
        assert_eq!(other.settings.edit.grid_size, 4.0);
        frame_map(&mut other, egui::vec2(1000.0, 800.0));
        assert!((other.cameras.cam3d.pos - DVec3::new(100.0, 200.0, 300.0)).length() < 1e-6);
        assert!((other.cameras.cam3d.yaw_deg - 45.0).abs() < 1e-6);
    }

    #[test]
    fn unsaved_question_names_the_operation() {
        hp_app::register_translations();
        let open = PendingOp::Open(PathBuf::from("C:/maps/next.vmf")).unsaved_question("a.vmf");
        let new = PendingOp::New.unsaved_question("a.vmf");
        let exit = PendingOp::Exit.unsaved_question("a.vmf");
        assert!(open.contains("next.vmf"), "{open}");
        assert_ne!(new, exit);
        assert_ne!(open, exit);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hammerpro_files_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn translations() {
        hp_app::register_translations();
        hp_app::i18n::register(crate::app::TR);
    }

    #[derive(Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
    struct Note {
        text: String,
    }

    impl hp_app::sidecar::SidecarSection for Note {
        const NAME: &'static str = "test_note";
    }

    #[test]
    fn the_sidecar_follows_the_map() {
        translations();
        let dir = scratch("sidecar");
        let cache = dir.join("cache");
        let mut state = hp_app::testing::test_state();
        state.sidecar = Sidecar::open(Some(&cache), None);
        state.sidecar.set(&Note { text: "first".into() });
        let map = dir.join("room.vmf");
        assert!(save_map_to(&mut state, &map));
        assert!(Sidecar::file_in(&cache, &map).is_file(), "written on save");
        state.sidecar.set(&Note { text: "second".into() });
        new_map(&mut state, egui::vec2(1000.0, 800.0));
        assert_eq!(state.sidecar.get::<Note>(), Note::default(), "a new map starts empty");
        assert!(open_map(&mut state, &map, egui::vec2(1000.0, 800.0)));
        assert_eq!(state.sidecar.get::<Note>().text, "second");
        assert_eq!(state.sidecar.map(), Some(map.as_path()));
        let copy = dir.join("copy.vmf");
        assert!(save_map_to(&mut state, &copy));
        assert_eq!(Sidecar::open(Some(&cache), Some(&copy)).get::<Note>().text, "second");
        std::fs::write(Sidecar::file_in(&cache, &map), "[[ not toml").unwrap();
        assert!(open_map(&mut state, &map, egui::vec2(1000.0, 800.0)));
        assert_eq!(state.sidecar.get::<Note>(), Note::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_view_comes_back_with_the_map() {
        translations();
        let dir = scratch("view_state");
        let cache = dir.join("cache");
        let mut state = hp_app::testing::test_state();
        state.sidecar = Sidecar::open(Some(&cache), None);
        let map = dir.join("room.vmf");
        assert!(save_map_to(&mut state, &map));
        state.cameras.cam3d.pos = DVec3::new(10.0, 20.0, 30.0);
        state.cameras.cam3d.yaw_deg = 33.0;
        state.cameras.top.zoom = 2.5;
        state.cameras.top.center = glam::DVec2::new(64.0, -32.0);
        state.tool = hp_app::ToolId::Block;
        new_map(&mut state, egui::vec2(1000.0, 800.0));
        assert_ne!(state.cameras.top.zoom, 2.5);
        assert!(open_map(&mut state, &map, egui::vec2(1000.0, 800.0)));
        assert_eq!(state.cameras.cam3d.pos, DVec3::new(10.0, 20.0, 30.0));
        assert_eq!(state.cameras.cam3d.yaw_deg, 33.0);
        assert_eq!(state.cameras.top.zoom, 2.5);
        assert_eq!(state.cameras.top.center, glam::DVec2::new(64.0, -32.0));
        assert_eq!(state.sidecar.get::<ViewState>().tool(), Some(hp_app::ToolId::Block));
        let mut bad = ViewState::capture(&state);
        bad.cam3d = Some(([f64::NAN, 0.0, 0.0], 0.0, 0.0));
        bad.top = Some(hp_app::view::Camera2d { center: glam::DVec2::ZERO, zoom: 0.0 });
        bad.tool = Some("NoSuchTool".to_owned());
        let before = state.cameras;
        bad.apply_cameras(&mut state);
        assert_eq!(state.cameras.cam3d.pos, before.cam3d.pos);
        assert_eq!(state.cameras.top.zoom, before.top.zoom);
        assert_eq!(bad.tool(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn toasts(state: &mut AppState) -> Vec<(Severity, String)> {
        state.toasts.live().iter().map(|t| (t.severity, t.text.clone())).collect()
    }

    fn console_warnings(state: &AppState) -> Vec<String> {
        state.console.lines().filter(|l| l.severity == Severity::Warning).map(|l| l.text.clone()).collect()
    }

    fn write_damaged(dir: &Path) -> PathBuf {
        let text = hp_vmf::write(&hp_vmf::parse(&grouped_vmf()).expect("parses"));
        let cut = text.find("\"plane\"").expect("plane") + 12;
        let path = dir.join("cut.vmf");
        std::fs::write(&path, &text.as_bytes()[..cut]).expect("write");
        path
    }

    #[test]
    fn importing_keeps_groups_visgroups_hidden_objects_and_the_encoding() {
        translations();
        let dir = scratch("import");
        let mut map = hp_vmf::parse(&grouped_vmf()).expect("parses");
        let light = map.entities.iter_mut().find(|e| e.classname == "light").expect("light");
        light.hidden = true;
        light.set("targetname", "caf\u{e9}_light");
        map.world.solids[0].hidden = true;
        let bytes: Vec<u8> = hp_vmf::write(&map).chars().map(|c| if c == '\u{e9}' { 0xE9 } else { c as u8 }).collect();
        assert!(std::str::from_utf8(&bytes).is_err(), "the file is not UTF-8");
        let path = dir.join("prefab.vmf");
        std::fs::write(&path, &bytes).expect("write");

        let mut state = hp_app::testing::test_state();
        let (solids, groups) = (state.doc.scene().solids().len(), state.doc.scene().groups().len());
        assert_eq!(import_map_from(&mut state, &path), Some(5));
        let scene = state.doc.scene();
        assert_eq!(scene.solids().len(), solids + 3, "two world solids and the trigger's");
        assert_eq!(scene.groups().len(), groups + 2, "both groups are recreated");
        let names: Vec<&str> = scene.visgroups().iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"Floor") && names.contains(&"Parent"), "{names:?}");
        let parent = scene.visgroups().iter().find(|v| v.name == "Parent").expect("Parent");
        assert_eq!(parent.children[0].name, "Lights");
        let imported = scene
            .entities()
            .values()
            .map(|n| &n.entity)
            .find(|e| e.classname == "light" && e.id != 5)
            .expect("imported light");
        assert!(imported.hidden, "hidden objects stay hidden");
        assert_eq!(imported.get("targetname"), Some("caf\u{e9}_light"), "the text encoding is kept");
        let floor = scene.visgroups().iter().find(|v| v.name == "Floor").expect("Floor").id;
        let in_floor = scene.solids().values().filter(|n| n.solid.editor.visgroup_ids == vec![floor]).count();
        assert_eq!(in_floor, 2, "solids keep their visgroups");
        let hidden = scene.solids().values().filter(|n| n.solid.hidden).count();
        assert_eq!(hidden, 1, "hidden brushes stay hidden");
        let toast = toasts(&mut state).pop().expect("toast");
        assert_eq!(toast, (Severity::Success, tf!("app.imported_many", 5, "prefab.vmf")));
        assert!(state.doc.undo());
        assert_eq!(state.doc.scene().solids().len(), solids);
        assert_eq!(state.doc.scene().groups().len(), groups);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_errors_are_reported() {
        translations();
        let dir = scratch("import_errors");
        let mut state = hp_app::testing::test_state();
        let solids = state.doc.scene().solids().len();
        let missing = dir.join("missing.vmf");
        assert_eq!(import_map_from(&mut state, &missing), None);
        let (severity, text) = toasts(&mut state).pop().expect("toast");
        assert_eq!(severity, Severity::Error);
        let failed = tf!("app.import_failed", "missing.vmf", "");
        assert!(text.starts_with(&failed), "{text}");
        let empty = dir.join("empty.vmf");
        std::fs::write(&empty, hp_vmf::write(&hp_vmf::Map::new_empty())).expect("write");
        assert_eq!(import_map_from(&mut state, &empty), None);
        assert_eq!(toasts(&mut state).pop().expect("toast"), (Severity::Info, tf!("app.import_empty", "empty.vmf")));
        assert_eq!(state.doc.scene().solids().len(), solids);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_map_opens_without_a_path_and_warns() {
        translations();
        let dir = scratch("damaged");
        let path = write_damaged(&dir);
        let mut state = hp_app::testing::test_state();
        assert!(open_map(&mut state, &path, egui::vec2(1000.0, 800.0)));
        assert!(state.doc.path().is_none(), "Ctrl+S asks for a new name");
        assert!(is_damaged(&state.doc));
        assert!(state.doc.is_dirty());
        assert_eq!(map_display_name(&state), tf!("app.title_damaged", "cut.vmf"));
        assert!(map_display_name(&state).contains("cut.vmf"));
        assert_eq!(
            toasts(&mut state),
            vec![(Severity::Warning, tf!("app.load_damaged", "cut.vmf"))],
            "a warning instead of the green Opened toast"
        );
        let lines = load_warning_lines(&state.doc, hp_app::i18n::lang());
        assert!(!lines.is_empty());
        assert_eq!(console_warnings(&state), lines);
        assert_eq!(lines.len(), state.doc.load_warnings().len());
        let french = load_warning_lines(&state.doc, Lang::Fr);
        assert_eq!(french.len(), lines.len());
        for line in &french {
            assert!(!state.doc.load_warnings().contains(line), "not translated: {line}");
        }
        assert!(french.last().is_some_and(|l| l.contains("Enregistrer sous")), "{french:?}");
        let english = load_warning_lines(&state.doc, Lang::En);
        assert_eq!(english.last(), Some(&tr_args("app.load.damaged_note", Lang::En, &[])));
        let saved = dir.join("fixed.vmf");
        state.doc.save(&saved).expect("save");
        assert_eq!(map_display_name(&state), "fixed.vmf");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_as_proposes_a_name_based_on_the_damaged_file() {
        translations();
        let dir = scratch("save_as_name");
        let path = write_damaged(&dir);
        let mut state = hp_app::testing::test_state();
        assert_eq!(default_save_name(&state.doc), "untitled.vmf", "a new map");
        assert!(open_map(&mut state, &path, egui::vec2(1000.0, 800.0)));
        assert!(state.doc.path().is_none());
        assert_eq!(default_save_name(&state.doc), "cut_recovered.vmf");
        std::fs::write(dir.join("cut_recovered.vmf"), "x").expect("write");
        assert_eq!(default_save_name(&state.doc), "cut_recovered_2.vmf");
        let saved = dir.join("fixed.vmf");
        assert!(save_map_to(&mut state, &saved));
        assert_eq!(default_save_name(&state.doc), "fixed.vmf");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_autosave_never_takes_the_original_path() {
        translations();
        let dir = scratch("damaged_autosave");
        let autosave = write_damaged(&dir);
        let original = dir.join("map.vmf");
        let mut state = hp_app::testing::test_state();
        assert!(open_recovered(&mut state, &autosave, Some(&original), egui::vec2(1000.0, 800.0)));
        assert!(state.doc.path().is_none(), "saving must not overwrite the original with a cut map");
        let toasts = toasts(&mut state);
        assert!(!toasts.is_empty() && toasts.iter().all(|(s, _)| *s == Severity::Warning), "{toasts:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn clean_and_slightly_broken_maps() {
        translations();
        let dir = scratch("clean");
        let clean = dir.join("clean.vmf");
        std::fs::write(&clean, hp_app::testing::TEST_VMF).expect("write");
        let mut state = hp_app::testing::test_state();
        assert!(open_map(&mut state, &clean, egui::vec2(1000.0, 800.0)));
        assert_eq!(state.doc.path(), Some(clean.as_path()));
        assert_eq!(map_display_name(&state), "clean.vmf");
        let shown = toasts(&mut state);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].0, Severity::Success);
        assert!(console_warnings(&state).is_empty());

        let stray = dir.join("stray.vmf");
        std::fs::write(&stray, format!("{}}}\n", hp_app::testing::TEST_VMF)).expect("write");
        let mut state = hp_app::testing::test_state();
        assert!(open_map(&mut state, &stray, egui::vec2(1000.0, 800.0)));
        assert_eq!(state.doc.path(), Some(stray.as_path()), "not damaged: Save writes the file");
        assert_eq!(toasts(&mut state), vec![(Severity::Warning, tf!("app.load_problems_one", "stray.vmf"))]);
        assert_eq!(console_warnings(&state), vec![tr_args("app.load.stray_one", hp_app::i18n::lang(), &[])]);
        assert_eq!(
            load_warning_lines(&state.doc, Lang::Fr),
            vec!["1 accolade fermante inattendue a été ignorée".to_owned()]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_kind_of_load_problem_is_translated() {
        translations();
        for n in [1, 2] {
            let mut report = LoadReport::default();
            assert!(report_lines(&report, Lang::Fr).is_empty());
            report.syntax.unterminated_string = true;
            report.syntax.unclosed_blocks = n;
            report.syntax.stray_closes = n;
            report.syntax.missing_values = n;
            report.unreadable_values = n;
            report.unreadable_examples = vec!["solid 12, side 631: plane \"(-1.#IND 0 0)\"".to_owned()];
            report.missing_ids = n;
            let english = report.warnings();
            for lang in [Lang::En, Lang::Fr] {
                let lines = report_lines(&report, lang);
                assert_eq!(lines.len(), english.len(), "one line per kind of problem");
                for line in &lines {
                    assert!(!line.contains("{0}") && !line.contains("{1}"), "unfilled placeholder: {line}");
                }
                assert!(lines.iter().any(|l| l.contains("(-1.#IND 0 0)")), "the examples are kept");
            }
            let french = report_lines(&report, Lang::Fr);
            assert!(french.iter().all(|l| !english.contains(l)), "{french:?}");
            if n == 2 {
                assert!(french.iter().any(|l| l.starts_with("2 accolades fermantes")), "{french:?}");
            }
        }
    }
}
