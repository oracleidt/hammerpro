use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};

use egui::{Align2, Color32, CursorIcon, Modifiers, Pos2, RichText, Ui};
use glam::DVec3;
use hp_app::foliage::is_paint_surface as is_surface;
use hp_app::i18n::TrTable;
use hp_app::settings::{Settings, SprinkleMode, SprinkleSettings};
use hp_app::state::AppState;
use hp_app::{Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, t, tf};
use hp_editor::prefab::SprinkleItem;
use hp_editor::{Id, ObjRef, PickOptions};
use hp_geom::Ray;
use hp_render::Overlay;

use crate::common::{Mouse, Ov, fresh, paint_label, rgba};
use crate::sprinkle_def::{ScanCache, ScanSources, SprinkleLibrary, SprinkleType, default_dirs, scan};

const MAX_CELLS: usize = 40_000;
const CIRCLE_SEGMENTS: usize = 64;
const MAX_MARKERS: usize = 600;
const COLOR_BRUSH: Color32 = Color32::from_rgb(120, 230, 140);
const COLOR_ERASE: Color32 = Color32::from_rgb(255, 110, 100);
const MIN_DEPTH: f64 = 64.0;
const WHEEL_STEP: f64 = 1.15;

pub const SPRINKLE_TR: TrTable = &[
    ("sprinkle.type", "Type:", "Type :"),
    ("sprinkle.no_types", "No definition", "Aucune définition"),
    (
        "sprinkle.type_tip",
        "Sprinkle definitions: scripts/hammer/sprinkle/*.txt of the game, the Hammer++ folder and the Hammer Pro folder",
        "Définitions sprinkle : scripts/hammer/sprinkle/*.txt du jeu, le dossier de Hammer++ et le dossier de Hammer Pro",
    ),
    ("sprinkle.refresh_tip", "Read the definitions again", "Relire les définitions"),
    (
        "sprinkle.open_source_tip",
        "Open source: the definition file in the text editor",
        "Ouvrir la source : le fichier de définition dans l'éditeur de texte",
    ),
    (
        "sprinkle.open_source_archive",
        "This definition is inside an archive",
        "Cette définition se trouve dans une archive",
    ),
    ("sprinkle.folder_tip", "Open the Hammer Pro sprinkle folder", "Ouvrir le dossier sprinkle de Hammer Pro"),
    ("sprinkle.loading", "Reading definitions...", "Lecture des définitions..."),
    ("sprinkle.mode.add", "Add", "Ajouter"),
    ("sprinkle.mode.subtract", "Subtract", "Retirer"),
    ("sprinkle.mode.replace", "Replace", "Remplacer"),
    ("sprinkle.mode.overwrite", "Overwrite", "Écraser"),
    ("sprinkle.mode.add_tip", "Fill empty cells", "Remplir les cases vides"),
    ("sprinkle.mode.subtract_tip", "Remove entities of the type", "Retirer les entités du type"),
    (
        "sprinkle.mode.replace_tip",
        "Pick a new entry for the entities of the type",
        "Tirer une nouvelle entrée pour les entités du type",
    ),
    (
        "sprinkle.mode.overwrite_tip",
        "Empty the cells, then fill them again",
        "Vider les cases, puis les remplir à nouveau",
    ),
    ("sprinkle.density", "Density:", "Densité :"),
    (
        "sprinkle.density_tip",
        "Chance that a cell gets an entity (or loses it when subtracting)",
        "Probabilité qu'une case reçoive une entité (ou la perde en retirant)",
    ),
    ("sprinkle.radius", "Radius:", "Rayon :"),
    ("sprinkle.radius_tip", "Brush radius (Alt + mouse wheel)", "Rayon du pinceau (Alt + molette)"),
    ("sprinkle.use_grid", "Use grid", "Utiliser la grille"),
    (
        "sprinkle.use_grid_tip",
        "Use the editor grid instead of the grid of the definition ({0})",
        "Utiliser la grille de l'éditeur au lieu de la grille de la définition ({0})",
    ),
    ("sprinkle.random_yaw", "Random yaw", "Lacet aléatoire"),
    ("sprinkle.offset", "Offset:", "Décalage :"),
    (
        "sprinkle.offset_tip",
        "Random move inside the cell, in percent of half a cell",
        "Déplacement aléatoire dans la case, en pourcentage d'une demi-case",
    ),
    ("sprinkle.z_offset", "Z offset:", "Décalage Z :"),
    ("sprinkle.z_offset_tip", "Distance from the surface", "Distance à la surface"),
    ("sprinkle.align", "Align to normal", "Aligner sur la normale"),
    (
        "sprinkle.align_tip",
        "Tilt entities to follow the surface they rest on",
        "Incliner les entités pour suivre la surface sur laquelle elles reposent",
    ),
    (
        "sprinkle.hint",
        "{0}: drag to sprinkle, Ctrl + drag to erase, Alt + wheel to change the radius",
        "{0} : glissez pour répandre, Ctrl + glisser pour effacer, Alt + molette pour changer le rayon",
    ),
    (
        "sprinkle.hint_none",
        "No sprinkle definition: add .txt files to {0}",
        "Aucune définition sprinkle : ajoutez des fichiers .txt dans {0}",
    ),
    ("sprinkle.pick_type", "Choose a sprinkle type first", "Choisissez d'abord un type sprinkle"),
    (
        "sprinkle.too_many_cells",
        "The grid is too small for this radius: nothing was sprinkled",
        "La grille est trop petite pour ce rayon : rien n'a été répandu",
    ),
    ("sprinkle.failed", "Sprinkle failed: {0}", "Échec du sprinkle : {0}"),
    (
        "sprinkle.warnings",
        "{0} problems in the definitions (see the console)",
        "{0} problèmes dans les définitions (voir la console)",
    ),
];

fn mode_key(mode: SprinkleMode) -> (&'static str, &'static str) {
    match mode {
        SprinkleMode::Add => ("sprinkle.mode.add", "sprinkle.mode.add_tip"),
        SprinkleMode::Subtract => ("sprinkle.mode.subtract", "sprinkle.mode.subtract_tip"),
        SprinkleMode::Replace => ("sprinkle.mode.replace", "sprinkle.mode.replace_tip"),
        SprinkleMode::Overwrite => ("sprinkle.mode.overwrite", "sprinkle.mode.overwrite_tip"),
    }
}

pub fn user_sprinkle_dir() -> std::path::PathBuf {
    Settings::config_dir().join("sprinkle")
}

fn rand01(seed: u64, a: i64, b: i64, c: u64) -> f64 {
    let mut x = seed
        ^ (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ c.wrapping_mul(0x1656_67B1_9E37_79F9);
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

fn plane_axes(k: usize) -> (usize, usize) {
    match k {
        0 => (1, 2),
        1 => (0, 2),
        _ => (0, 1),
    }
}

fn dominant_axis(n: DVec3) -> usize {
    let a = n.abs();
    if a.z >= a.x && a.z >= a.y {
        2
    } else if a.x >= a.y {
        0
    } else {
        1
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Brush {
    pub center: DVec3,
    pub normal: DVec3,
    pub axis: usize,
    pub sign: f64,
    pub radius: f64,
    pub column: bool,
}

impl Brush {
    pub(crate) fn surface(point: DVec3, normal: DVec3, radius: f64) -> Option<Self> {
        let n = normal.try_normalize().filter(|n| n.is_finite())?;
        let axis = dominant_axis(n);
        let sign = if n[axis] < 0.0 { -1.0 } else { 1.0 };
        point.is_finite().then_some(Self { center: point, normal: n, axis, sign, radius, column: false })
    }

    pub(crate) fn column(view: &ViewInfo, pos: Pos2, radius: f64) -> Option<Self> {
        let (_, _, k) = view.kind.axes_2d()?;
        let n = view.kind.toward_viewer();
        let center = view.kind.from_2d(view.screen_to_2d(pos), 0.0);
        let sign = if n[k] < 0.0 { -1.0 } else { 1.0 };
        center.is_finite().then_some(Self { center, normal: n, axis: k, sign, radius, column: true })
    }

    fn depth(&self) -> f64 {
        self.radius.max(MIN_DEPTH)
    }

    fn unit(k: usize) -> DVec3 {
        let mut v = DVec3::ZERO;
        v[k] = 1.0;
        v
    }

    fn covers(&self, p: DVec3) -> bool {
        let (a, b) = plane_axes(self.axis);
        let (da, db) = (p[a] - self.center[a], p[b] - self.center[b]);
        if da * da + db * db > self.radius * self.radius {
            return false;
        }
        self.column || (p[self.axis] - self.center[self.axis]).abs() <= self.depth()
    }

    fn outline(&self) -> Vec<DVec3> {
        let (a, b) = plane_axes(self.axis);
        let (ea, eb) = (Self::unit(a), Self::unit(b));
        let lift = if self.column { DVec3::ZERO } else { Self::unit(self.axis) * self.sign * 0.5 };
        (0..CIRCLE_SEGMENTS)
            .map(|i| {
                let t = i as f64 / CIRCLE_SEGMENTS as f64 * std::f64::consts::TAU;
                self.center + lift + (ea * t.cos() + eb * t.sin()) * self.radius
            })
            .collect()
    }

    fn ray(&self, pa: f64, pb: f64) -> (Ray, f64) {
        let (a, b) = plane_axes(self.axis);
        let mut origin = self.center;
        origin[a] = pa;
        origin[b] = pb;
        let dir = Self::unit(self.axis) * -self.sign;
        if self.column {
            origin[self.axis] = self.sign * 65_536.0;
            (Ray::new(origin, dir), f64::INFINITY)
        } else {
            origin[self.axis] = self.center[self.axis] + self.sign * self.depth();
            (Ray::new(origin, dir), self.depth() * 2.0)
        }
    }
}

fn cell_of(x: f64, step: f64) -> i64 {
    let c = (x / step).round();
    if c.is_finite() { c.clamp(-1e15, 1e15) as i64 } else { 0 }
}

fn brush_cells(brush: &Brush, grid: [f64; 2]) -> Option<Vec<(i64, i64)>> {
    let (a, b) = plane_axes(brush.axis);
    let (ca, cb, r) = (brush.center[a], brush.center[b], brush.radius);
    let (i0, i1) = (((ca - r) / grid[0]).floor(), ((ca + r) / grid[0]).ceil());
    let (j0, j1) = (((cb - r) / grid[1]).floor(), ((cb + r) / grid[1]).ceil());
    if !(i0.is_finite() && i1.is_finite() && j0.is_finite() && j1.is_finite()) {
        return None;
    }
    let count = (i1 - i0 + 1.0) * (j1 - j0 + 1.0);
    if count > MAX_CELLS as f64 * 1.3 {
        return None;
    }
    let mut out = Vec::new();
    for i in i0 as i64..=i1 as i64 {
        for j in j0 as i64..=j1 as i64 {
            let (da, db) = (i as f64 * grid[0] - ca, j as f64 * grid[1] - cb);
            if da * da + db * db <= r * r {
                out.push((i, j));
            }
        }
    }
    (out.len() <= MAX_CELLS).then_some(out)
}

struct Stroke {
    kind: Arc<SprinkleType>,
    erase: bool,
    mode: SprinkleMode,
    owned: HashMap<Id, DVec3>,
    cells: HashSet<(usize, i64, i64)>,
    rolled: HashSet<Id>,
    seed: u64,
    view: usize,
    last: Option<Pos2>,
    added: usize,
    removed: usize,
    warned: bool,
}

#[derive(Debug, Default)]
pub(crate) struct Changes {
    pub add: Vec<SprinkleItem>,
    pub remove: Vec<Id>,
    pub too_many: bool,
}

#[derive(Clone, Copy, Debug)]
struct Hover {
    view: usize,
    frame: u64,
    brush: Brush,
    erase: bool,
}

pub struct SprinkleTool {
    library: Arc<SprinkleLibrary>,
    scanning: Option<Receiver<SprinkleLibrary>>,
    cache: Arc<Mutex<ScanCache>>,
    scanned: Option<u64>,
    hover: Option<Hover>,
    stroke: Option<Stroke>,
    seed: u64,
}

impl Default for SprinkleTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SprinkleTool {
    pub fn new() -> Self {
        Self {
            library: Arc::new(SprinkleLibrary::default()),
            scanning: None,
            cache: Arc::new(Mutex::new(ScanCache::default())),
            scanned: None,
            hover: None,
            stroke: None,
            seed: 0x5EED_0000_0001,
        }
    }

    pub fn library(&self) -> &SprinkleLibrary {
        &self.library
    }

    pub fn set_library(&mut self, library: SprinkleLibrary) {
        self.library = Arc::new(library);
        self.scanning = None;
    }

    pub fn is_scanning(&self) -> bool {
        self.scanning.is_some()
    }

    pub fn is_painting(&self) -> bool {
        self.stroke.is_some()
    }

    pub fn refresh(&mut self, state: &AppState, repaint: Option<&egui::Context>) {
        let fs = state.assets.fs().cloned();
        let root = state.settings.game.gmod_root.clone().or_else(|| fs.as_ref().and_then(|f| f.root().map(Into::into)));
        let mut dirs = default_dirs(root.as_deref(), &user_sprinkle_dir());
        dirs.extend(state.settings.placement.sprinkle.extra_dirs.iter().cloned());
        let sources = ScanSources { fs, dirs };
        let cache = Arc::clone(&self.cache);
        let (tx, rx) = std::sync::mpsc::channel();
        let repaint = repaint.cloned();
        let spawned = std::thread::Builder::new().name("hp-sprinkle-scan".to_owned()).spawn(move || {
            let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
            let lib = scan(&sources, &mut cache);
            let _ = tx.send(lib);
            if let Some(ctx) = repaint {
                ctx.request_repaint();
            }
        });
        match spawned {
            Ok(_) => self.scanning = Some(rx),
            Err(e) => log::warn!("cannot read sprinkle definitions: {e}"),
        }
        self.scanned = Some(state.assets.generation());
    }

    fn poll(&mut self, state: &mut AppState, repaint: Option<&egui::Context>) {
        if let Some(rx) = &self.scanning {
            match rx.try_recv() {
                Ok(lib) => {
                    self.scanning = None;
                    for w in &lib.warnings {
                        log::warn!("sprinkle: {w}");
                    }
                    if !lib.warnings.is_empty() {
                        state.log(Severity::Warning, tf!("sprinkle.warnings", lib.warnings.len()));
                        for w in &lib.warnings {
                            state.log(Severity::Warning, w.clone());
                        }
                    }
                    let s = &mut state.settings.placement.sprinkle;
                    if lib.find(&s.kind).is_none()
                        && let Some(first) = lib.types.first()
                    {
                        s.kind.clone_from(&first.name);
                        state.settings_dirty = true;
                    }
                    self.library = Arc::new(lib);
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.scanning = None,
            }
        }
        let wanted = std::mem::take(&mut state.runtime.placement.sprinkle_refresh)
            || self.scanned.is_none_or(|g| g != state.assets.generation());
        if wanted && self.scanning.is_none() {
            self.refresh(state, repaint);
        }
    }

    fn kind(&self, state: &AppState) -> Option<Arc<SprinkleType>> {
        self.library.find(&state.settings.placement.sprinkle.kind).cloned()
    }

    fn grid(state: &AppState, kind: &SprinkleType) -> [f64; 2] {
        if state.settings.placement.sprinkle.use_grid {
            let g = state.grid_size();
            let g = if g.is_finite() && g >= 1.0 { g } else { 1.0 };
            [g, g]
        } else {
            kind.grid_or_default()
        }
    }

    fn brush_at(state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<Brush> {
        let radius = state.settings.placement.sprinkle.radius();
        if view.is_2d() {
            return Brush::column(view, pos, radius);
        }
        let (o, d) = view.ray(pos);
        if !o.is_finite() || !d.is_finite() {
            return None;
        }
        let scene = state.doc.scene();
        let filter = |obj: ObjRef| is_surface(scene, obj);
        let opts = PickOptions { include_entities: false, filter: Some(&filter), ..PickOptions::default() };
        let hit = state.doc.pick_ray(&Ray::new(o, d), &opts, &|e| state.entity_bounds(e))?;
        Brush::surface(hit.point, hit.normal, radius)
    }

    fn begin_stroke(&mut self, state: &mut AppState, view: usize, erase: bool) -> bool {
        let Some(kind) = self.kind(state) else {
            state.notify(Severity::Info, t!("sprinkle.pick_type"));
            return false;
        };
        let owned: HashMap<Id, DVec3> = state
            .doc
            .scene()
            .entities()
            .values()
            .filter(|n| kind.owns(&n.entity))
            .filter(|n| !state.doc.scene().is_brush_entity(n.entity.id))
            .filter(|n| state.doc.is_visible(ObjRef::Entity(n.entity.id)))
            .filter_map(|n| n.entity.origin().filter(|o| o.is_finite()).map(|o| (n.entity.id, o)))
            .collect();
        self.seed = self.seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        state.doc.begin("Sprinkle");
        self.stroke = Some(Stroke {
            kind,
            erase,
            mode: state.settings.placement.sprinkle.mode,
            owned,
            cells: HashSet::new(),
            rolled: HashSet::new(),
            seed: self.seed,
            view,
            last: None,
            added: 0,
            removed: 0,
            warned: false,
        });
        true
    }

    fn end_stroke(&mut self, state: &mut AppState) {
        if let Some(s) = self.stroke.take() {
            state.doc.commit();
            if s.added + s.removed > 0 {
                log::debug!("sprinkle stroke: {} added, {} removed", s.added, s.removed);
            }
        }
    }

    fn paint(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2) {
        let Some(last) = self.stroke.as_ref().map(|s| s.last) else { return };
        let points: Vec<Pos2> = match last {
            Some(prev) => {
                let radius_px = (state.settings.placement.sprinkle.radius() / view.units_per_point().max(1e-6)) as f32;
                let step = (radius_px * 0.25).max(2.0);
                let n = ((prev.distance(pos) / step).ceil() as usize).clamp(1, 32);
                (1..=n).map(|i| prev.lerp(pos, i as f32 / n as f32)).collect()
            }
            None => vec![pos],
        };
        for p in points {
            let Some(brush) = Self::brush_at(state, view, p) else { continue };
            self.apply(state, &brush);
        }
        if let Some(s) = &mut self.stroke {
            s.last = Some(pos);
        }
    }

    pub(crate) fn apply(&mut self, state: &mut AppState, brush: &Brush) {
        let Some(stroke) = &mut self.stroke else { return };
        let opts = state.settings.placement.sprinkle.clone();
        let grid = Self::grid(state, &stroke.kind);
        let changes = compute_changes(state, stroke, brush, grid, &opts);
        if changes.too_many && !stroke.warned {
            stroke.warned = true;
            state.notify(Severity::Warning, t!("sprinkle.too_many_cells"));
        }
        if changes.add.is_empty() && changes.remove.is_empty() {
            return;
        }
        match state.doc.sprinkle(&changes.add, &changes.remove) {
            Ok(ids) => {
                for id in &changes.remove {
                    if stroke.owned.remove(id).is_some() {
                        stroke.removed += 1;
                    }
                }
                for (id, item) in ids.iter().zip(&changes.add) {
                    stroke.owned.insert(*id, item.origin);
                }
                stroke.added += ids.len();
            }
            Err(e) => state.notify(Severity::Error, tf!("sprinkle.failed", e)),
        }
    }

    fn mode_ui(ui: &mut Ui, s: &mut SprinkleSettings) -> bool {
        let mut changed = false;
        for mode in SprinkleMode::ALL {
            let (label, tip) = mode_key(mode);
            if ui.selectable_label(s.mode == mode, t!(label)).on_hover_text(t!(tip)).clicked() && s.mode != mode {
                s.mode = mode;
                changed = true;
            }
        }
        changed
    }
}

fn compute_changes(
    state: &AppState,
    stroke: &mut Stroke,
    brush: &Brush,
    grid: [f64; 2],
    opts: &SprinkleSettings,
) -> Changes {
    let mut out = Changes::default();
    let chance = opts.chance();
    let (a, b) = plane_axes(brush.axis);
    let reach = brush.radius + grid[0].max(grid[1]);
    let mut by_cell: HashMap<(i64, i64), Vec<Id>> = HashMap::new();
    let mut under: Vec<Id> = Vec::new();
    for (id, p) in &stroke.owned {
        if (p[a] - brush.center[a]).abs() > reach || (p[b] - brush.center[b]).abs() > reach {
            continue;
        }
        let near = brush.column || (p[brush.axis] - brush.center[brush.axis]).abs() <= brush.depth();
        if !near {
            continue;
        }
        if brush.covers(*p) {
            under.push(*id);
        }
        by_cell.entry((cell_of(p[a], grid[0]), cell_of(p[b], grid[1]))).or_default().push(*id);
    }
    under.sort_unstable();
    if stroke.erase {
        out.remove = under;
        return out;
    }
    if stroke.mode == SprinkleMode::Subtract {
        for id in under {
            if stroke.rolled.insert(id) && rand01(stroke.seed, i64::from(id), 0, 7) < chance {
                out.remove.push(id);
            }
        }
        return out;
    }
    let Some(cells) = brush_cells(brush, grid) else {
        out.too_many = true;
        return out;
    };
    let scene = state.doc.scene();
    let filter = |obj: ObjRef| is_surface(scene, obj);
    let pick_opts = PickOptions { include_entities: false, filter: Some(&filter), ..PickOptions::default() };
    let jitter = opts.jitter();
    let k = brush.axis;
    for (i, j) in cells {
        if !stroke.cells.insert((k, i, j)) {
            continue;
        }
        let occupants = by_cell.get(&(i, j)).map(Vec::as_slice).unwrap_or(&[]);
        let place = match stroke.mode {
            SprinkleMode::Add => occupants.is_empty() && rand01(stroke.seed, i, j, 1) < chance,
            SprinkleMode::Replace => !occupants.is_empty(),
            SprinkleMode::Overwrite => rand01(stroke.seed, i, j, 1) < chance,
            SprinkleMode::Subtract => false,
        };
        if stroke.mode == SprinkleMode::Overwrite {
            out.remove.extend_from_slice(occupants);
        }
        if !place {
            continue;
        }
        let pa = i as f64 * grid[0] + (rand01(stroke.seed, i, j, 2) - 0.5) * grid[0] * jitter;
        let pb = j as f64 * grid[1] + (rand01(stroke.seed, i, j, 3) - 0.5) * grid[1] * jitter;
        let (ray, max_distance) = brush.ray(pa, pb);
        let opts_ray = PickOptions { max_distance, ..pick_opts };
        let Some(hit) = state.doc.pick_ray(&ray, &opts_ray, &|e| state.entity_bounds(e)) else { continue };
        let normal = hit.normal.try_normalize().unwrap_or(brush.normal);
        if !hit.point.is_finite() {
            continue;
        }
        let Some(entry) = stroke.kind.pick(rand01(stroke.seed, i, j, 4)) else { continue };
        let mut kv = stroke.kind.entity_keyvalues(entry);
        let Some(class) = kv.iter().position(|(k, _)| k.eq_ignore_ascii_case("classname")).map(|p| kv.remove(p).1)
        else {
            continue;
        };
        let lift = if opts.align_to_normal { normal } else { Brush::unit(k) * brush.sign };
        let origin = hit.point + lift * opts.z_offset();
        let base = kv
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("angles"))
            .and_then(|(_, v)| hp_vmf::parse_vec3(v))
            .filter(|a| a.is_finite())
            .unwrap_or(DVec3::ZERO);
        let mut angles = base;
        if opts.random_yaw {
            angles.y = (rand01(stroke.seed, i, j, 5) * 360.0).floor();
        }
        if opts.align_to_normal {
            angles = hp_geom::align_angles_to_normal(angles, normal);
        }
        let angles = hp_geom::clean_angles(angles);
        let text = format!("{} {} {}", fmt(angles.x), fmt(angles.y), fmt(angles.z));
        match kv.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case("angles")) {
            Some(slot) => slot.1 = text,
            None => kv.push(("angles".to_owned(), text)),
        }
        out.add.push(SprinkleItem { classname: class, origin, keyvalues: kv });
        if stroke.mode == SprinkleMode::Replace {
            out.remove.extend_from_slice(occupants);
        }
    }
    out.remove.sort_unstable();
    out.remove.dedup();
    out
}

fn fmt(x: f64) -> String {
    let r = (x * 100.0).round() / 100.0;
    let r = if r == 0.0 { 0.0 } else { r };
    let s = format!("{r:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

impl Tool for SprinkleTool {
    fn id(&self) -> ToolId {
        ToolId::Sprinkle
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.end_stroke(state);
        self.hover = None;
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
        self.poll(state, Some(&input.response.ctx));
        if input.navigating {
            self.hover = None;
            return ToolFlow::Ignored;
        }
        let m = Mouse::from_input(input);
        self.handle_mouse(state, input.view, &m)
    }

    fn wheel(&mut self, state: &mut AppState, _view: &ViewInfo, notches: f32, modifiers: Modifiers) -> ToolFlow {
        if !modifiers.alt || modifiers.ctrl || !notches.is_finite() {
            return ToolFlow::Ignored;
        }
        let s = &mut state.settings.placement.sprinkle;
        let r = s.radius() * WHEEL_STEP.powf(f64::from(notches.clamp(-10.0, 10.0)));
        s.radius = r.clamp(*SprinkleSettings::RADIUS_RANGE.start(), *SprinkleSettings::RADIUS_RANGE.end()).round();
        state.settings_dirty = true;
        ToolFlow::Handled
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.stroke.is_some() && !ctx.input(|i| i.pointer.primary_down()) {
            self.end_stroke(state);
        }
        false
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let Some(h) = self.hover.filter(|h| fresh(state, h.frame)) else { return };
        if h.brush.column && h.view != view.index {
            return;
        }
        let color = if h.erase { COLOR_ERASE } else { COLOR_BRUSH };
        let mut ov = Ov::new(out, view);
        ov.polygon_outline(&h.brush.outline(), rgba(color, 0.95), true);
        if !h.brush.column {
            let tip = h.brush.center + h.brush.normal * h.brush.radius.min(64.0) * 0.5;
            ov.xray_line(h.brush.center, tip, rgba(color, 0.9));
        }
        if h.view == view.index
            && !h.erase
            && let Some(kind) = self.kind(state)
            && let Some(cells) = brush_cells(&h.brush, Self::grid(state, &kind))
            && cells.len() <= MAX_MARKERS
        {
            let grid = Self::grid(state, &kind);
            let (a, b) = plane_axes(h.brush.axis);
            for (i, j) in cells {
                let mut p = h.brush.center;
                p[a] = i as f64 * grid[0];
                p[b] = j as f64 * grid[1];
                ov.point(p, rgba(color, 0.8), 3.0);
            }
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some(h) = self.hover.filter(|h| fresh(state, h.frame) && h.view == view.index) else { return };
        let Some(kind) = self.kind(state) else { return };
        let Some(s) = view.world_to_screen(h.brush.center) else { return };
        if !view.rect.contains(s) {
            return;
        }
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let color = if h.erase { COLOR_ERASE } else { COLOR_BRUSH };
        paint_label(&painter, s + egui::vec2(12.0, -12.0), Align2::LEFT_BOTTOM, &kind.name, color);
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.poll(state, Some(ui.ctx()));
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().slider_width = 52.0;
            ui.label(t!("sprinkle.type")).on_hover_text(t!("sprinkle.type_tip"));
            let current = self.kind(state);
            let text = match &current {
                Some(k) => RichText::new(k.name.as_str()),
                None if self.scanning.is_some() => RichText::new(t!("sprinkle.loading")).weak().italics(),
                None => RichText::new(t!("sprinkle.no_types")).weak().italics(),
            };
            let lib = Arc::clone(&self.library);
            egui::ComboBox::from_id_salt("hp_sprinkle_type").selected_text(text).width(170.0).show_ui(ui, |ui| {
                for k in &lib.types {
                    let selected = current.as_ref().is_some_and(|c| Arc::ptr_eq(c, k));
                    let resp = ui.selectable_label(selected, k.name.as_str()).on_hover_text(k.source.label());
                    if resp.clicked() && !selected {
                        state.settings.placement.sprinkle.kind.clone_from(&k.name);
                        changed = true;
                    }
                }
            });
            if ui.small_button("🔄").on_hover_text(t!("sprinkle.refresh_tip")).clicked() {
                self.refresh(state, Some(ui.ctx()));
            }
            let disk = current.as_ref().and_then(|k| k.source.disk_path().map(std::path::Path::to_path_buf));
            let tip = if current.is_some() && disk.is_none() {
                "sprinkle.open_source_archive"
            } else {
                "sprinkle.open_source_tip"
            };
            if ui
                .add_enabled(disk.is_some(), egui::Button::new("✏").small())
                .on_hover_text(t!(tip))
                .on_disabled_hover_text(t!(tip))
                .clicked()
            {
                state.runtime.placement.open_path = disk;
            }
            if ui.small_button("📂").on_hover_text(t!("sprinkle.folder_tip")).clicked() {
                state.runtime.placement.open_path = Some(user_sprinkle_dir());
            }
            ui.separator();
            let s = &mut state.settings.placement.sprinkle;
            changed |= Self::mode_ui(ui, s);
            ui.separator();
            ui.label(t!("sprinkle.density"));
            changed |= ui
                .add(egui::Slider::new(&mut s.density, 0.0..=100.0).suffix("%").integer())
                .on_hover_text(t!("sprinkle.density_tip"))
                .changed();
            ui.label(t!("sprinkle.radius"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.radius).range(SprinkleSettings::RADIUS_RANGE).speed(2.0))
                .on_hover_text(t!("sprinkle.radius_tip"))
                .changed();
            ui.separator();
            let def_grid = current.as_ref().map_or([64.0, 64.0], |k| k.grid_or_default());
            changed |= ui
                .checkbox(&mut s.use_grid, t!("sprinkle.use_grid"))
                .on_hover_text(tf!("sprinkle.use_grid_tip", format!("{} x {}", def_grid[0], def_grid[1])))
                .changed();
            changed |= ui.checkbox(&mut s.random_yaw, t!("sprinkle.random_yaw")).changed();
            ui.label(t!("sprinkle.offset"));
            changed |= ui
                .add(egui::Slider::new(&mut s.random_offset, 0.0..=100.0).suffix("%").integer())
                .on_hover_text(t!("sprinkle.offset_tip"))
                .changed();
            ui.label(t!("sprinkle.z_offset"));
            changed |= ui
                .add(egui::DragValue::new(&mut s.z_offset).range(-4096.0..=4096.0).speed(0.5))
                .on_hover_text(t!("sprinkle.z_offset_tip"))
                .changed();
            changed |= ui
                .checkbox(&mut s.align_to_normal, t!("sprinkle.align"))
                .on_hover_text(t!("sprinkle.align_tip"))
                .changed();
        });
        if changed {
            state.settings_dirty = true;
        }
    }

    fn cursor(&self, _state: &AppState, _view: &ViewInfo, _pos: Pos2) -> Option<CursorIcon> {
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        Some(match self.kind(state) {
            Some(k) => tf!("sprinkle.hint", k.name),
            None => tf!("sprinkle.hint_none", user_sprinkle_dir().display()),
        })
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        if self.stroke.take().is_some() {
            state.doc.cancel();
            return true;
        }
        self.hover = None;
        false
    }
}

impl SprinkleTool {
    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        let Some(pos) = m.pos else {
            self.hover = None;
            return ToolFlow::Ignored;
        };
        let erase = m.modifiers.ctrl || m.modifiers.command;
        let brush = Self::brush_at(state, view, pos);
        self.hover = brush.map(|brush| Hover { view: view.index, frame: state.frame.index, brush, erase });
        if m.drag_started || (m.clicked && self.stroke.is_none()) {
            self.end_stroke(state);
            if self.begin_stroke(state, view.index, erase) {
                self.paint(state, view, m.origin().unwrap_or(pos));
                self.paint(state, view, pos);
            }
        } else if m.dragging && self.stroke.as_ref().is_some_and(|s| s.view == view.index) {
            self.paint(state, view, pos);
        }
        if m.clicked || m.drag_stopped {
            if m.drag_stopped && self.stroke.is_some() {
                self.paint(state, view, pos);
            }
            self.end_stroke(state);
        }
        if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sprinkle_def::{SprinkleSource, parse_file};
    use crate::tests_support::{info_2d, info_3d_looking_down, settle};
    use hp_app::ViewKind;

    const DEFS: &str = r#""Rocks"
{
	base { "classname" "prop_static" grid "32 32" }
	1 { "model" "models/rock01.mdl" }
	1 { "model" "models/rock02.mdl" "angles" "0 90 0" }
}
"Nodes" { base { "classname" "info_node" } 1 { } }
"#;

    fn tool(state: &mut AppState) -> SprinkleTool {
        let (types, _) = parse_file(DEFS, &SprinkleSource::Disk("defs.txt".into()));
        let mut t = SprinkleTool::new();
        t.set_library(SprinkleLibrary { types: types.into_iter().map(Arc::new).collect(), ..Default::default() });
        t.scanned = Some(state.assets.generation());
        state.settings.placement.sprinkle.kind = "Rocks".to_owned();
        t
    }

    fn rocks(state: &AppState) -> Vec<hp_vmf::Entity> {
        state
            .doc
            .scene()
            .entities()
            .values()
            .filter(|n| n.entity.classname == "prop_static")
            .map(|n| n.entity.clone())
            .collect()
    }

    fn floor_state() -> AppState {
        settle()
    }

    #[test]
    fn a_stroke_fills_cells_once_and_is_one_undo_step() {
        let mut state = floor_state();
        let mut t = tool(&mut state);
        {
            let s = &mut state.settings.placement.sprinkle;
            s.density = 100.0;
            s.radius = 64.0;
            s.random_offset = 0.0;
            s.random_yaw = false;
        }
        let view = info_3d_looking_down(DVec3::new(-128.0, -128.0, 800.0));
        let p = view.world_to_screen(DVec3::new(-128.0, -128.0, 0.0)).expect("screen");
        let undo = state.doc.history_len().0;
        t.handle_mouse(&mut state, &view, &Mouse::drag_start(p, p));
        assert!(t.is_painting());
        let placed = rocks(&state);
        assert!(placed.len() > 9, "{}", placed.len());
        for e in &placed {
            let o = e.origin().expect("origin");
            assert_eq!(o.z, 0.0, "{o:?}");
            assert_eq!(o.x % 32.0, 0.0);
            assert!(e.get("model").is_some_and(|m| m.starts_with("models/rock0")));
        }
        let n = placed.len();
        t.handle_mouse(&mut state, &view, &Mouse::drag(p, p));
        assert_eq!(rocks(&state).len(), n);
        let q = view.world_to_screen(DVec3::new(-64.0, -160.0, 0.0)).expect("screen");
        t.handle_mouse(&mut state, &view, &Mouse::drag(p, q));
        let after = rocks(&state).len();
        assert!(after > n);
        t.handle_mouse(&mut state, &view, &Mouse::drag_end(p, q));
        assert!(!t.is_painting());
        assert_eq!(state.doc.history_len().0, undo + 1, "one undo step");
        assert_eq!(state.doc.undo_label(), Some("Sprinkle"));
        t.handle_mouse(&mut state, &view, &Mouse::drag_start(p, p));
        t.handle_mouse(&mut state, &view, &Mouse::drag_end(p, p));
        assert_eq!(rocks(&state).len(), after);
        let ctrl = Modifiers::CTRL;
        t.handle_mouse(&mut state, &view, &Mouse::drag_start(p, p).with_modifiers(ctrl));
        t.handle_mouse(&mut state, &view, &Mouse::drag_end(p, p).with_modifiers(ctrl));
        let left = rocks(&state).len();
        assert!(left < after && left > 0, "{left} of {after}");
        assert!(state.doc.undo());
        assert_eq!(rocks(&state).len(), after);
    }

    #[test]
    fn modes_and_options() {
        let mut state = floor_state();
        let mut t = tool(&mut state);
        {
            let s = &mut state.settings.placement.sprinkle;
            s.density = 100.0;
            s.radius = 100.0;
            s.random_yaw = true;
            s.align_to_normal = true;
            s.z_offset = 4.0;
        }
        let view = info_2d(ViewKind::Top, 1.0);
        let p = view.world_to_screen(DVec3::new(-150.0, -150.0, 0.0)).expect("p");
        t.handle_mouse(&mut state, &view, &Mouse::click(p));
        let first = rocks(&state);
        assert!(!first.is_empty());
        for e in &first {
            let o = e.origin().expect("origin");
            assert_eq!(o.z, 4.0, "rays from the viewer find the floor, plus the Z offset");
            let a = e.angles().expect("angles");
            assert!(a.y.is_finite() && a.x == 0.0);
        }
        state.settings.placement.sprinkle.mode = SprinkleMode::Replace;
        t.handle_mouse(&mut state, &view, &Mouse::click(p));
        assert_eq!(rocks(&state).len(), first.len());
        state.settings.placement.sprinkle.mode = SprinkleMode::Overwrite;
        state.settings.placement.sprinkle.density = 0.0;
        t.handle_mouse(&mut state, &view, &Mouse::click(p));
        assert!(rocks(&state).is_empty());
        state.settings.placement.sprinkle.mode = SprinkleMode::Add;
        state.settings.placement.sprinkle.density = 100.0;
        t.handle_mouse(&mut state, &view, &Mouse::click(p));
        assert_eq!(rocks(&state).len(), first.len());
        state.settings.placement.sprinkle.mode = SprinkleMode::Subtract;
        t.handle_mouse(&mut state, &view, &Mouse::click(p));
        assert!(rocks(&state).is_empty());
        assert!(state.doc.scene().entities().values().any(|n| n.entity.classname == "light"));
    }

    #[test]
    fn escape_drops_the_stroke_and_wheel_resizes() {
        let mut state = floor_state();
        let mut t = tool(&mut state);
        state.settings.placement.sprinkle.density = 100.0;
        let view = info_2d(ViewKind::Top, 1.0);
        let p = view.world_to_screen(DVec3::new(-150.0, -150.0, 0.0)).expect("p");
        let count = state.doc.scene().entities().len();
        t.handle_mouse(&mut state, &view, &Mouse::drag_start(p, p));
        assert!(state.doc.scene().entities().len() > count);
        assert!(t.cancel(&mut state));
        assert_eq!(state.doc.scene().entities().len(), count);
        assert!(!state.doc.in_transaction());
        let r = state.settings.placement.sprinkle.radius();
        assert_eq!(t.wheel(&mut state, &view, 1.0, Modifiers::ALT), ToolFlow::Handled);
        assert!(state.settings.placement.sprinkle.radius() > r);
        assert_eq!(t.wheel(&mut state, &view, 1.0, Modifiers::NONE), ToolFlow::Ignored);
        state.settings.placement.sprinkle.kind = "nothing".to_owned();
        t.handle_mouse(&mut state, &view, &Mouse::click(p));
        assert!(!state.doc.in_transaction());
    }

    #[test]
    fn translations() {
        let problems = hp_app::i18n::lint_table(SPRINKLE_TR);
        assert!(problems.is_empty(), "{problems:#?}");
        let src = include_str!("sprinkle.rs");
        for (k, en, fr) in SPRINKLE_TR {
            assert!(k.starts_with("sprinkle."), "{k}");
            assert!(src.matches(&format!("\"{k}\"")).count() >= 2, "unused key {k}");
            for i in 0..3 {
                let p = format!("{{{i}}}");
                assert_eq!(en.contains(&p), fr.contains(&p), "{k}: {p}");
            }
        }
        let glyphs_ctx = egui::Context::default();
        let mut out = glyphs_ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        let font = egui::FontId::proportional(14.0);
        for g in ["🔄", "📂", "✏", ToolId::Sprinkle.icon()] {
            assert!(glyphs_ctx.fonts_mut(|f| f.has_glyphs(&font, g)), "missing glyph {g}");
        }
    }

    #[test]
    fn brush_geometry() {
        let b = Brush::surface(DVec3::new(0.0, 0.0, 10.0), DVec3::new(0.1, 0.0, 1.0), 64.0).expect("brush");
        assert_eq!((b.axis, b.sign), (2, 1.0));
        assert!(b.covers(DVec3::new(30.0, 30.0, 0.0)));
        assert!(!b.covers(DVec3::new(60.0, 60.0, 0.0)));
        assert!(!b.covers(DVec3::new(0.0, 0.0, 200.0)));
        let wall = Brush::surface(DVec3::ZERO, DVec3::NEG_X, 32.0).expect("wall");
        assert_eq!((wall.axis, wall.sign), (0, -1.0));
        assert_eq!(brush_cells(&wall, [32.0, 32.0]).map(|c| c.len()), Some(5));
        let huge = Brush { radius: 4096.0, ..wall };
        assert!(brush_cells(&huge, [1.0, 1.0]).is_none());
        assert!(Brush::surface(DVec3::NAN, DVec3::Z, 1.0).is_none());
        assert!(Brush::surface(DVec3::ZERO, DVec3::ZERO, 1.0).is_none());
        assert_eq!(fmt(90.0), "90");
        assert_eq!(fmt(-0.001), "0");
        assert_eq!(fmt(12.345), "12.35");
        let r = rand01(1, 2, 3, 4);
        assert!((0.0..1.0).contains(&r));
        assert_eq!(r, rand01(1, 2, 3, 4));
    }

    #[test]
    fn draws_without_definitions() {
        let mut state = settle();
        let mut t = SprinkleTool::new();
        t.scanned = Some(state.assets.generation());
        assert!(t.status_hint(&state).is_some_and(|h| !h.is_empty()));
        hp_app::testing::run_ui(&mut state, |ui, state| t.options_ui(ui, state));
        let view = info_3d_looking_down(DVec3::new(0.0, 0.0, 500.0));
        t.handle_mouse(&mut state, &view, &Mouse::hover(view.rect.center()));
        let mut out = Overlay::default();
        t.overlay(&state, &view, &mut out);
        assert!(!out.lines.is_empty() || !out.lines_on_top.is_empty());
    }
}
