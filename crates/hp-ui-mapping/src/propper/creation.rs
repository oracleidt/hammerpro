use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use egui::{RichText, Ui};
use glam::DVec3;
use hp_app::runtime::{
    CollisionElement, PropCreationRequest, PropperCollisionCommand, PropperCollisionInfo, PropperPieceInfo,
};
use hp_app::settings::PropperCollisionMode;
use hp_app::{AppState, t, tf, theme};
use hp_editor::prop_export::{CollisionEditError, EditableCollision, PropDirectives, PropGeometry, collect_prop};
use hp_geom::Aabb;
use hp_render::{Overlay, OverlayLine, OverlayPoint};

use super::lod_preview::LodPreview;
use super::viewport::{EditPoint, Scene, Viewport};
use crate::dialogs::MapDialog;
use crate::util::enter_pressed;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Collision {
    Brushes,
    Hull,
    Decompose,
    Box,
    Manual,
}

const VERTEX_KEY: usize = 1000;
const EDGE_KEY: usize = 100_000;
const FACE_KEY: usize = 200_000;

type Limits = (usize, usize);

fn element_key(element: CollisionElement) -> usize {
    match element {
        CollisionElement::Vertex => VERTEX_KEY,
        CollisionElement::Edge => EDGE_KEY,
        CollisionElement::Face => FACE_KEY,
    }
}

fn key_element(key: usize) -> Option<(CollisionElement, usize)> {
    match key {
        k if k >= FACE_KEY => Some((CollisionElement::Face, k - FACE_KEY)),
        k if k >= EDGE_KEY => Some((CollisionElement::Edge, k - EDGE_KEY)),
        k if k >= VERTEX_KEY => Some((CollisionElement::Vertex, k - VERTEX_KEY)),
        _ => None,
    }
}

fn element_points(piece: &EditableCollision, element: CollisionElement, index: usize) -> Option<Vec<usize>> {
    match element {
        CollisionElement::Vertex => (index < piece.points.len()).then(|| vec![index]),
        CollisionElement::Edge => piece.edges().get(index).map(|e| e.to_vec()),
        CollisionElement::Face => piece.faces().get(index).cloned(),
    }
}

fn element_count(piece: &EditableCollision, element: CollisionElement) -> usize {
    match element {
        CollisionElement::Vertex => piece.points.len(),
        CollisionElement::Edge => piece.edges().len(),
        CollisionElement::Face => piece.faces().len(),
    }
}

fn piece_info(piece: &EditableCollision) -> PropperPieceInfo {
    PropperPieceInfo {
        vertices: piece.points.iter().map(|p| p.to_array()).collect(),
        edges: piece.edges(),
        faces: piece.faces().to_vec(),
    }
}

fn snap_value(value: DVec3, grid: f64) -> DVec3 {
    if grid > 0.0 && grid.is_finite() { (value / grid).round() * grid } else { value }
}

fn edit_message(error: CollisionEditError) -> String {
    match error {
        CollisionEditError::Folded => t!("propper.collision_folded").to_owned(),
        CollisionEditError::Geometry(error) => error.to_string(),
    }
}

fn mode_of(mode: PropperCollisionMode) -> Collision {
    match mode {
        PropperCollisionMode::Brushes => Collision::Brushes,
        PropperCollisionMode::Hull => Collision::Hull,
        PropperCollisionMode::Decompose => Collision::Decompose,
    }
}
#[derive(Clone, Copy)]
enum Folder {
    Destination,
    Source,
}

#[derive(Default)]
pub(super) struct CreateDialog {
    draft: Option<Draft>,
    error: Option<String>,
    picker: Option<(Folder, mpsc::Receiver<Option<PathBuf>>)>,
    #[cfg(test)]
    last_rect: Option<egui::Rect>,
}

struct Draft {
    source_serial: u64,
    ids: Vec<hp_editor::Id>,
    geometry: Arc<PropGeometry>,
    model: String,
    destination: String,
    source: String,
    material_folder: String,
    surface: String,
    surfaces: Vec<String>,
    surface_search: String,
    reuse_materials: bool,
    collision: Collision,
    manual: Option<Vec<EditableCollision>>,
    hull: Option<(usize, EditableCollision)>,
    decomposed: Option<(Limits, Vec<EditableCollision>)>,
    boxed: EditableCollision,
    piece: usize,
    element: CollisionElement,
    grab: Option<(usize, [u64; 3], Vec<usize>)>,
    published: Option<(u64, String, String, Option<String>)>,
    custom_illum: bool,
    illum: DVec3,
    custom_bbox: bool,
    bbox: Aabb,
    bounds: Aabb,
    show_collision: bool,
    show_bbox: bool,
    show_illum: bool,
    tab: usize,
    viewport: Viewport,
    lods: LodPreview,
    revision: u64,
    overlay: Option<(u64, Arc<Overlay>)>,
    error: Option<String>,
}

pub(super) fn model_path(input: &str) -> Option<String> {
    let normalized = input.trim().replace('\\', "/").to_ascii_lowercase();
    let normalized = normalized.strip_prefix("models/").unwrap_or(&normalized);
    let normalized = normalized.strip_suffix(".mdl").unwrap_or(normalized);
    (!normalized.is_empty()
        && normalized.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
        }))
    .then(|| normalized.to_owned())
}

pub(super) fn name_prefix(name: &str) -> String {
    let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit());
    if trimmed.is_empty() { "prop".to_owned() } else { trimmed.to_owned() }
}

pub(super) fn split_model(model: &str) -> (String, String) {
    match model_path(model) {
        Some(path) => match path.rsplit_once('/') {
            Some((folder, name)) => (folder.to_owned(), name.to_owned()),
            None => (String::new(), path),
        },
        None => (String::new(), String::new()),
    }
}

pub(super) fn next_model(folder: &str, prefix: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let folder = folder.trim().replace('\\', "/").trim_matches('/').to_ascii_lowercase();
    let prefix = name_prefix(&prefix.trim().to_ascii_lowercase());
    let join = |name: String| if folder.is_empty() { name } else { format!("{folder}/{name}") };
    (1..100_000)
        .map(|index| join(format!("{prefix}{index:02}")))
        .find(|candidate| !taken(candidate))
        .unwrap_or_else(|| join(prefix.clone()))
}

fn taken_models(state: &AppState, destination: &str) -> impl Fn(&str) -> bool + use<> {
    let used: std::collections::HashSet<String> = state
        .doc
        .scene()
        .entities()
        .values()
        .filter_map(|node| node.entity.get("model").and_then(model_path))
        .collect();
    let fs = state.assets.fs().cloned();
    let root = PathBuf::from(destination.trim());
    move |candidate: &str| {
        let file = format!("models/{candidate}.mdl");
        used.contains(candidate)
            || (root.is_absolute() && root.join(&file).is_file())
            || fs.as_ref().is_some_and(|fs| fs.exists(&file))
    }
}

fn remembered_dir(saved: &str, fallback: PathBuf) -> String {
    let saved = saved.trim();
    if !saved.is_empty() && PathBuf::from(saved).is_absolute() {
        saved.to_owned()
    } else {
        fallback.to_string_lossy().into_owned()
    }
}

fn follows_model(material_folder: &str, model: &str) -> bool {
    model_path(model).is_some_and(|model| {
        material_folder.trim().replace('\\', "/").trim_matches('/').eq_ignore_ascii_case(&format!("models/{model}"))
    })
}

fn solid_bounds(mut bounds: Aabb) -> Aabb {
    for axis in 0..3 {
        if bounds.max[axis] - bounds.min[axis] < 0.01 {
            let center = (bounds.min[axis] + bounds.max[axis]) * 0.5;
            bounds.min[axis] = center - 0.5;
            bounds.max[axis] = center + 0.5;
        }
    }
    bounds
}

fn corner(bounds: Aabb, index: usize) -> DVec3 {
    DVec3::new(
        if index & 1 == 0 { bounds.min.x } else { bounds.max.x },
        if index & 2 == 0 { bounds.min.y } else { bounds.max.y },
        if index & 4 == 0 { bounds.min.z } else { bounds.max.z },
    )
}

fn vector(ui: &mut Ui, label: &str, value: &mut DVec3) -> bool {
    let mut changed = false;
    ui.label(label);
    ui.horizontal(|ui| {
        for (axis, prefix) in ["X ", "Y ", "Z "].into_iter().enumerate() {
            changed |=
                ui.add(egui::DragValue::new(&mut value[axis]).prefix(prefix).speed(0.25).max_decimals(4)).changed();
        }
    });
    changed
}

pub fn prop_geometry(state: &mut AppState, ids: &[hp_editor::Id]) -> Result<PropGeometry, String> {
    let names: Vec<String> = ids
        .iter()
        .filter_map(|id| state.doc.scene().solid(*id))
        .flat_map(|node| node.solid.sides.iter().map(|side| side.material.clone()))
        .collect();
    let mut sizes = std::collections::HashMap::new();
    for name in names {
        let size = state
            .assets
            .material_info(&name)
            .filter(|info| info.width > 0 && info.height > 0)
            .map(|info| (info.width, info.height))
            .or_else(|| state.assets.fs().and_then(|fs| fs.material_texture_size(&name)))
            .unwrap_or((512, 512));
        sizes.insert(name.to_ascii_lowercase(), size);
    }
    collect_prop(&state.doc, ids, &|name| sizes.get(&name.to_ascii_lowercase()).copied().unwrap_or((512, 512)))
        .ok_or_else(|| t!("propper.empty").to_owned())
}

impl Draft {
    fn new(state: &mut AppState) -> Result<Self, String> {
        let mut ids = state.doc.selected_solid_ids();
        ids.sort_unstable();
        if ids.is_empty() {
            return Err(t!("propper.create_select").into());
        }
        let geometry = Arc::new(prop_geometry(state, &ids)?);
        let bounds = Aabb::from_points(geometry.tris.iter().flat_map(|tri| tri.pos));
        let mut hash = 0x811c_9dc5u32;
        for byte in ids.iter().flat_map(|id| id.to_le_bytes()) {
            hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
        }
        let stem = state.doc.path().and_then(|path| path.file_stem()).map_or("map".into(), |s| s.to_string_lossy());
        let stem: String = stem.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(40).collect();
        let destination = state
            .assets
            .fs()
            .and_then(|fs| fs.game_dir())
            .map(PathBuf::from)
            .or_else(|| state.settings.game.gmod_root.as_ref().map(|root| root.join("garrysmod")))
            .unwrap_or_else(|| hp_app::settings::Settings::cache_dir().join("prop_exports"));
        let lab = &state.settings.lab;
        let destination = remembered_dir(&lab.propper_export_dir, destination);
        let source = remembered_dir(&lab.propper_source_dir, hp_app::settings::Settings::cache_dir().join("propsrc"));
        let model = if lab.propper_model_folder.trim().is_empty() && lab.propper_name_prefix.trim().is_empty() {
            format!("hammerpro/{stem}_{hash:08x}")
        } else {
            next_model(&lab.propper_model_folder, &lab.propper_name_prefix, &taken_models(state, &destination))
        };
        let lab = &state.settings.lab;
        let material_folder = if lab.propper_material_dir.trim().is_empty() {
            format!("models/{model}")
        } else {
            lab.propper_material_dir.trim().to_owned()
        };
        let surfaces = state.assets.fs().map_or_else(|| vec!["default".into()], |fs| fs.surface_properties());
        let surface = surfaces
            .iter()
            .find(|name| !lab.propper_surface.is_empty() && name.eq_ignore_ascii_case(&lab.propper_surface))
            .cloned()
            .unwrap_or_default();
        let mut draft = Self {
            source_serial: state.doc.revision(),
            ids,
            geometry,
            material_folder,
            model,
            destination,
            source,
            surface,
            surfaces,
            surface_search: String::new(),
            reuse_materials: lab.propper_reuse_materials,
            collision: Collision::Brushes,
            manual: None,
            hull: None,
            decomposed: None,
            boxed: EditableCollision::box_at(solid_bounds(bounds)).map_err(|e| e.to_string())?,
            piece: 0,
            element: CollisionElement::Vertex,
            grab: None,
            published: None,
            custom_illum: false,
            illum: bounds.center(),
            custom_bbox: false,
            bbox: solid_bounds(bounds),
            bounds,
            show_collision: true,
            show_bbox: true,
            show_illum: true,
            tab: 0,
            viewport: Viewport::default(),
            lods: LodPreview::default(),
            revision: 1,
            overlay: None,
            error: None,
        };
        let limits = state.settings.lab.collision_limits();
        let initial = mode_of(state.settings.lab.propper_collision_mode);
        if initial != Collision::Brushes {
            draft.choose_collision(initial, limits);
        }
        Ok(draft)
    }

    fn source_triangles(&self) -> Vec<[DVec3; 3]> {
        if self.geometry.pieces.is_empty() {
            self.geometry.tris.iter().map(|tri| tri.pos).collect()
        } else {
            self.geometry.pieces.iter().flatten().copied().collect()
        }
    }

    fn pieces(&self) -> Vec<&[[DVec3; 3]]> {
        match self.collision {
            Collision::Brushes => self.geometry.pieces.iter().map(Vec::as_slice).collect(),
            Collision::Hull => self.hull.iter().map(|(_, hull)| hull.triangles()).collect(),
            Collision::Decompose => {
                self.decomposed.iter().flat_map(|(_, pieces)| pieces.iter().map(EditableCollision::triangles)).collect()
            }
            Collision::Box => vec![self.boxed.triangles()],
            Collision::Manual => self.manual.iter().flatten().map(EditableCollision::triangles).collect(),
        }
    }

    fn generate(&mut self, mode: Collision, limits: Limits) -> Result<(), String> {
        match mode {
            Collision::Hull if self.hull.as_ref().is_none_or(|(key, _)| *key != limits.1) => {
                let points: Vec<DVec3> = self.source_triangles().into_iter().flatten().collect();
                let hull =
                    hp_geom::limited_hull(&points, limits.1).ok_or_else(|| t!("propper.physics_empty").to_owned())?;
                let hull = EditableCollision::new(hull).map_err(|e| e.to_string())?;
                self.hull = Some((limits.1, hull));
                self.revision += 1;
            }
            Collision::Decompose if self.decomposed.as_ref().is_none_or(|(key, _)| *key != limits) => {
                let options = hp_geom::DecomposeOptions {
                    max_pieces: limits.0,
                    max_hull_vertices: limits.1,
                    ..hp_geom::DecomposeOptions::default()
                };
                let pieces: Vec<EditableCollision> = hp_geom::decompose(&self.source_triangles(), options)
                    .into_iter()
                    .filter_map(|points| EditableCollision::new(points).ok())
                    .collect();
                if pieces.is_empty() {
                    return Err(t!("propper.physics_empty").to_owned());
                }
                self.decomposed = Some((limits, pieces));
                self.revision += 1;
            }
            _ => {}
        }
        Ok(())
    }

    fn choose_collision(&mut self, next: Collision, limits: Limits) {
        let prepared = match next {
            Collision::Manual if self.manual.is_none() => self
                .pieces()
                .into_iter()
                .map(EditableCollision::from_triangles)
                .collect::<Result<Vec<_>, _>>()
                .map(|pieces| self.manual = Some(pieces))
                .map_err(|e| e.to_string()),
            Collision::Hull | Collision::Decompose => self.generate(next, limits),
            _ => Ok(()),
        };
        match prepared {
            Ok(()) => {
                self.collision = next;
                self.error = None;
                self.revision += 1;
                self.viewport.selected = None;
                self.grab = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn manual_piece(&self) -> Option<&EditableCollision> {
        self.manual.as_ref().and_then(|pieces| pieces.get(self.piece))
    }

    fn move_element(&mut self, element: CollisionElement, index: usize, delta: DVec3) {
        let Some(piece) = self.manual.as_mut().and_then(|pieces| pieces.get_mut(self.piece)) else { return };
        let Some(indices) = element_points(piece, element, index) else {
            self.error = Some(t!("propper.collision_no_element").to_owned());
            return;
        };
        match piece.move_points(&indices, delta) {
            Ok(()) => {
                self.error = None;
                self.revision += 1;
            }
            Err(error) => self.error = Some(edit_message(error)),
        }
    }

    fn mode_id(&self, state: &AppState) -> &'static str {
        if !state.settings.lab.propper_options().collision {
            return "none";
        }
        match self.collision {
            Collision::Brushes => "brushes",
            Collision::Hull => "hull",
            Collision::Decompose => "decompose",
            Collision::Box => "box",
            Collision::Manual => "manual",
        }
    }

    fn set_mode(&mut self, state: &mut AppState, mode: &str) -> Result<(), String> {
        let lab = &mut state.settings.lab;
        if mode == "none" {
            if lab.propper_physics {
                return Err(t!("propper.physics_collision").to_owned());
            }
            lab.propper_collision = false;
            state.settings_dirty = true;
            self.revision += 1;
            return Ok(());
        }
        let next = match mode {
            "brushes" => Collision::Brushes,
            "hull" => Collision::Hull,
            "decompose" => Collision::Decompose,
            "box" => Collision::Box,
            "manual" => Collision::Manual,
            other => return Err(format!("unknown collision mode {other}")),
        };
        lab.propper_collision = true;
        match next {
            Collision::Brushes => lab.propper_collision_mode = PropperCollisionMode::Brushes,
            Collision::Hull => lab.propper_collision_mode = PropperCollisionMode::Hull,
            Collision::Decompose => lab.propper_collision_mode = PropperCollisionMode::Decompose,
            _ => {}
        }
        state.settings_dirty = true;
        let limits = state.settings.lab.collision_limits();
        self.choose_collision(next, limits);
        self.error.clone().map_or(Ok(()), Err)
    }

    fn run_commands(&mut self, state: &mut AppState) {
        let commands = std::mem::take(&mut state.runtime.lab.propper_collision_commands);
        for command in commands {
            let result = match command {
                PropperCollisionCommand::Mode(mode) => self.set_mode(state, &mode),
                PropperCollisionCommand::Move { piece, element, index, delta, snap } => {
                    if self.collision != Collision::Manual {
                        let limits = state.settings.lab.collision_limits();
                        self.choose_collision(Collision::Manual, limits);
                    }
                    if self.manual.as_ref().is_none_or(|pieces| piece >= pieces.len()) {
                        Err(t!("propper.collision_no_element").to_owned())
                    } else {
                        self.piece = piece;
                        self.element = element;
                        let delta = if snap { snap_value(delta, state.grid_size()) } else { delta };
                        self.viewport.selected = Some(element_key(element) + index);
                        self.move_element(element, index, delta);
                        self.error.clone().map_or(Ok(()), Err)
                    }
                }
                PropperCollisionCommand::AddBox => {
                    if self.collision != Collision::Manual {
                        let limits = state.settings.lab.collision_limits();
                        self.choose_collision(Collision::Manual, limits);
                    }
                    self.manual.get_or_insert_with(Vec::new).push(self.boxed.clone());
                    self.piece = self.manual.as_ref().map_or(0, |p| p.len() - 1);
                    self.revision += 1;
                    Ok(())
                }
                PropperCollisionCommand::RemovePiece(index) => match self.manual.as_mut() {
                    Some(pieces) if self.collision == Collision::Manual && index < pieces.len() => {
                        pieces.remove(index);
                        self.piece = self.piece.min(pieces.len().saturating_sub(1));
                        self.viewport.selected = None;
                        self.revision += 1;
                        Ok(())
                    }
                    _ => Err(t!("propper.collision_no_element").to_owned()),
                },
                PropperCollisionCommand::ModelName(name) => {
                    self.set_model(name);
                    Ok(())
                }
                PropperCollisionCommand::CdMaterials(folder) => {
                    self.material_folder = folder;
                    Ok(())
                }
            };
            if let Err(error) = result {
                self.error = Some(error);
            }
        }
    }

    fn publish(&mut self, state: &mut AppState) {
        let error = self.validation(state);
        let stamp = (
            self.revision * 4 + u64::from(state.settings.lab.propper_options().collision) * 2,
            self.model.clone(),
            self.material_folder.clone(),
            error.clone(),
        );
        if self.published.as_ref() == Some(&stamp) && state.runtime.lab.propper_collision_info.is_some() {
            return;
        }
        let pieces: Vec<PropperPieceInfo> = if self.collision == Collision::Manual {
            self.manual.iter().flatten().map(piece_info).collect()
        } else {
            self.pieces()
                .into_iter()
                .filter_map(|t| EditableCollision::from_triangles(t).ok())
                .map(|p| piece_info(&p))
                .collect()
        };
        let selected = self.viewport.selected.and_then(key_element).map(|(_, index)| index);
        state.runtime.lab.propper_collision_info = Some(PropperCollisionInfo {
            mode: self.mode_id(state).to_owned(),
            pieces,
            piece: self.piece,
            element: self.element.id().to_owned(),
            selected,
            modelname: model_path(&self.model).map_or_else(|| self.model.clone(), |m| format!("models/{m}.mdl")),
            cdmaterials: self.directives().cdmaterials,
            error,
        });
        self.published = Some(stamp);
    }

    fn directives(&self) -> PropDirectives {
        let folder = self.material_folder.trim().replace('\\', "/");
        PropDirectives {
            illumposition: self.custom_illum.then_some(self.illum),
            bbox: self.custom_bbox.then_some(self.bbox),
            cdmaterials: folder.strip_prefix("materials/").unwrap_or(&folder).trim_matches('/').to_owned(),
        }
    }

    fn validation(&self, state: &AppState) -> Option<String> {
        if self.source_serial != state.doc.revision() {
            return Some(t!("propper.source_changed").into());
        }
        if model_path(&self.model).is_none() {
            return Some(t!("propper.invalid_model_path").into());
        }
        if !PathBuf::from(&self.destination).is_absolute() || !PathBuf::from(&self.source).is_absolute() {
            return Some(t!("propper.absolute_directory").into());
        }
        if self.custom_illum && !self.illum.is_finite() {
            return Some(t!("propper.invalid_illum").into());
        }
        if self.custom_bbox
            && (!self.bbox.min.is_finite() || !self.bbox.max.is_finite() || !self.bbox.min.cmplt(self.bbox.max).all())
        {
            return Some(t!("propper.invalid_bounds").into());
        }
        if self.directives().validate().is_err() {
            return Some(t!("propper.invalid_material_path").into());
        }
        if !self.surface.is_empty() && !self.surfaces.iter().any(|surface| surface.eq_ignore_ascii_case(&self.surface))
        {
            return Some(t!("propper.invalid_surface").into());
        }
        if state.settings.lab.propper_options().collision && self.pieces().is_empty() {
            return Some(t!("propper.physics_empty").into());
        }
        self.error.clone()
    }

    fn request(&self, state: &AppState) -> PropCreationRequest {
        let mut geometry = (*self.geometry).clone();
        geometry.pieces = if state.settings.lab.propper_options().collision {
            self.pieces().iter().map(|piece| piece.to_vec()).collect()
        } else {
            Vec::new()
        };
        PropCreationRequest {
            source_serial: self.source_serial,
            solid_ids: self.ids.clone(),
            geometry,
            options: state.settings.lab.propper_options(),
            directives: self.directives(),
            model: model_path(&self.model).expect("validated model path"),
            source_dir: PathBuf::from(&self.source),
            destination: PathBuf::from(&self.destination),
            surface_prop: self.surface.clone(),
            reuse_materials: self.reuse_materials,
            lod: state.settings.lab.model_lod,
        }
    }

    fn remember(&self, lab: &mut hp_app::settings::LabSettings) {
        if model_path(&self.model).is_some() {
            let (folder, name) = split_model(&self.model);
            lab.remember_propper_folder(&folder);
            lab.propper_model_folder = folder;
            lab.propper_name_prefix = name_prefix(&name);
        }
        lab.propper_material_dir = if follows_model(&self.material_folder, &self.model) {
            String::new()
        } else {
            self.material_folder.trim().to_owned()
        };
        lab.propper_export_dir = self.destination.trim().to_owned();
        lab.propper_source_dir = self.source.trim().to_owned();
        lab.propper_reuse_materials = self.reuse_materials;
        lab.propper_surface = self.surface.clone();
    }

    fn set_model(&mut self, model: String) {
        if follows_model(&self.material_folder, &self.model)
            && let Some(path) = model_path(&model)
        {
            self.material_folder = format!("models/{path}");
        }
        self.model = model;
    }

    fn pick_folder(&mut self, folder: &str, state: &AppState) {
        let (_, name) = split_model(&self.model);
        let prefix = if name.is_empty() { state.settings.lab.propper_name_prefix.clone() } else { name };
        let next = next_model(folder, &prefix, &taken_models(state, &self.destination));
        self.set_model(next);
    }

    fn model_field(&mut self, ui: &mut Ui, state: &AppState) {
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 36.0).max(80.0);
            let mut typed = self.model.clone();
            if ui.add(egui::TextEdit::singleline(&mut typed).desired_width(width)).changed() {
                self.set_model(typed);
            }
            let recent = &state.settings.lab.propper_recent_folders;
            let mut chosen = None;
            ui.menu_button("▾", |ui| {
                ui.label(RichText::new(t!("propper.recent_folders")).weak());
                if recent.is_empty() {
                    ui.label(RichText::new(t!("propper.no_recent_folders")).small().weak());
                }
                for folder in recent {
                    if ui.button(format!("models/{folder}/")).clicked() {
                        chosen = Some(folder.clone());
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button(t!("propper.fresh_name")).clicked() {
                    chosen = Some(split_model(&self.model).0);
                    ui.close();
                }
            })
            .response
            .on_hover_text(t!("propper.recent_folders_tip"));
            if let Some(folder) = chosen {
                self.pick_folder(&folder, state);
            }
        });
    }

    fn controls(&mut self, ui: &mut Ui, state: &mut AppState) -> Option<Folder> {
        let mut folder = None;
        match self.tab {
            0 => {
                ui.label(t!("propper.model_path"));
                self.model_field(ui, state);
                ui.label(t!("propper.export_directory"));
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.destination);
                    if ui.button("…").clicked() {
                        folder = Some(Folder::Destination);
                    }
                });
                ui.label(t!("propper.source_directory"));
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.source);
                    if ui.button("…").clicked() {
                        folder = Some(Folder::Source);
                    }
                });
                ui.separator();
                let settings = &mut state.settings.lab;
                let mut changed = ui.checkbox(&mut settings.propper_physics, t!("propper.physics")).changed();
                if settings.propper_physics {
                    settings.propper_static = false;
                }
                ui.add_enabled_ui(!settings.propper_physics, |ui| {
                    changed |= ui.checkbox(&mut settings.propper_static, "$staticprop").changed()
                });
                let class = if settings.propper_physics {
                    "prop_physics"
                } else if settings.propper_static {
                    "prop_static"
                } else {
                    "prop_dynamic"
                };
                ui.label(RichText::new(class).weak());
                if settings.propper_physics {
                    ui.horizontal(|ui| {
                        ui.label(t!("propper.mass"));
                        changed |= ui
                            .add(egui::DragValue::new(&mut settings.propper_mass).speed(0.5).range(0.1..=50_000.0))
                            .changed();
                    });
                }
                ui.separator();
                changed |= hp_app::widgets::model_lod_ui(ui, &mut settings.model_lod);
                state.settings_dirty |= changed;
            }
            1 => {
                ui.label(t!("propper.collision_level"));
                let settings = &mut state.settings.lab;
                let previous = if !settings.propper_options().collision {
                    0
                } else if self.collision == Collision::Box {
                    1
                } else {
                    2
                };
                let mut level = previous;
                for (value, key) in
                    [(0, "propper.collision_none"), (1, "propper.collision_box"), (2, "propper.collision_precise")]
                {
                    ui.add_enabled_ui(!settings.propper_physics || value != 0, |ui| {
                        ui.radio_value(&mut level, value, t!(key));
                    });
                }
                let limits = settings.collision_limits();
                if level != previous {
                    settings.propper_collision = level != 0;
                    state.settings_dirty = true;
                    if level == 1 {
                        self.choose_collision(Collision::Box, limits);
                    }
                    if level == 2 && self.collision == Collision::Box {
                        let mode = mode_of(state.settings.lab.propper_collision_mode);
                        self.choose_collision(mode, limits);
                    }
                }
                if level == 2 {
                    let old = self.collision;
                    let mut next = old;
                    egui::CollapsingHeader::new(t!("propper.collision_advanced")).default_open(true).show(ui, |ui| {
                        for (mode, key, tip) in [
                            (Collision::Brushes, "propper.collision_brushes", "propper.collision_brushes_tip"),
                            (Collision::Hull, "propper.collision_hull", "propper.collision_hull_tip"),
                            (Collision::Decompose, "propper.collision_decompose", "propper.collision_decompose_tip"),
                            (Collision::Manual, "propper.collision_manual", "propper.collision_manual_tip"),
                        ] {
                            ui.radio_value(&mut next, mode, t!(key)).on_hover_text(t!(tip));
                        }
                    });
                    if next != old {
                        let lab = &mut state.settings.lab;
                        match next {
                            Collision::Brushes => lab.propper_collision_mode = PropperCollisionMode::Brushes,
                            Collision::Hull => lab.propper_collision_mode = PropperCollisionMode::Hull,
                            Collision::Decompose => lab.propper_collision_mode = PropperCollisionMode::Decompose,
                            _ => {}
                        }
                        state.settings_dirty = true;
                        self.choose_collision(next, limits);
                    }
                    if matches!(self.collision, Collision::Hull | Collision::Decompose) {
                        let lab = &mut state.settings.lab;
                        let mut changed = false;
                        if self.collision == Collision::Decompose {
                            ui.horizontal(|ui| {
                                ui.label(t!("propper.max_pieces"));
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut lab.collision_max_pieces)
                                            .range(1..=hp_app::settings::LabSettings::MAX_COLLISION_PIECES),
                                    )
                                    .on_hover_text(t!("propper.max_pieces_tip"))
                                    .changed();
                            });
                        }
                        ui.horizontal(|ui| {
                            ui.label(t!("propper.max_vertices"));
                            changed |= ui
                                .add(egui::DragValue::new(&mut lab.collision_max_hull_vertices).range(
                                    hp_app::settings::LabSettings::MIN_HULL_VERTICES
                                        ..=hp_app::settings::LabSettings::MAX_HULL_VERTICES,
                                ))
                                .on_hover_text(t!("propper.max_vertices_tip"))
                                .changed();
                        });
                        if changed {
                            state.settings_dirty = true;
                        }
                        let limits = state.settings.lab.collision_limits();
                        if !ui.input(|i| i.pointer.any_down())
                            && let Err(error) = self.generate(self.collision, limits)
                        {
                            self.error = Some(error);
                        }
                    }
                }
                if level == 2 && self.collision == Collision::Manual {
                    let count = self.manual.as_ref().map_or(0, Vec::len);
                    self.piece = self.piece.min(count.saturating_sub(1));
                    let previous_piece = self.piece;
                    egui::ComboBox::from_id_salt("propper_collision_piece")
                        .selected_text(format!("{} {} / {count}", t!("propper.piece"), self.piece + 1))
                        .show_ui(ui, |ui| {
                            for index in 0..count {
                                ui.selectable_value(
                                    &mut self.piece,
                                    index,
                                    format!("{} {}", t!("propper.piece"), index + 1),
                                );
                            }
                        });
                    if previous_piece != self.piece {
                        self.revision += 1;
                        self.viewport.selected = None;
                    }
                    ui.horizontal(|ui| {
                        if ui.button(t!("propper.add_box")).clicked() {
                            self.manual.get_or_insert_with(Vec::new).push(self.boxed.clone());
                            self.piece = self.manual.as_ref().unwrap().len() - 1;
                            self.revision += 1;
                        }
                        if ui.add_enabled(count > 0, egui::Button::new(t!("propper.remove_piece"))).clicked() {
                            self.manual.as_mut().unwrap().remove(self.piece);
                            self.viewport.selected = None;
                            self.revision += 1;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(t!("propper.select_element"));
                        let before = self.element;
                        for (element, key) in [
                            (CollisionElement::Vertex, "propper.element_vertex"),
                            (CollisionElement::Edge, "propper.element_edge"),
                            (CollisionElement::Face, "propper.element_face"),
                        ] {
                            ui.radio_value(&mut self.element, element, t!(key));
                        }
                        if before != self.element {
                            self.viewport.selected = None;
                            self.grab = None;
                            self.revision += 1;
                        }
                    });
                    let grid = state.grid_size();
                    let lab = &mut state.settings.lab;
                    if ui
                        .checkbox(&mut lab.propper_collision_snap, tf!("propper.collision_snap", grid))
                        .on_hover_text(t!("propper.collision_snap_tip"))
                        .changed()
                    {
                        state.settings_dirty = true;
                    }
                    let element = self.element;
                    if let Some(piece) = self.manual_piece().cloned() {
                        let total = element_count(&piece, element);
                        let base = element_key(element);
                        let mut index = self
                            .viewport
                            .selected
                            .and_then(key_element)
                            .filter(|(kind, _)| *kind == element)
                            .map_or(0, |(_, index)| index)
                            .min(total.saturating_sub(1));
                        let name = match element {
                            CollisionElement::Vertex => t!("propper.vertex"),
                            CollisionElement::Edge => t!("propper.element_edge"),
                            CollisionElement::Face => t!("propper.element_face"),
                        };
                        egui::ComboBox::from_id_salt("propper_collision_vertex")
                            .selected_text(format!("{name} {} / {total}", index + 1))
                            .show_ui(ui, |ui| {
                                for item in 0..total {
                                    ui.selectable_value(&mut index, item, (item + 1).to_string());
                                }
                            });
                        if total > 0 {
                            self.viewport.selected = Some(base + index);
                            let center = element_points(&piece, element, index).and_then(|ids| piece.center_of(&ids));
                            if let Some(mut position) = center {
                                let label = if element == CollisionElement::Vertex {
                                    t!("propper.vertex_position")
                                } else {
                                    t!("propper.element_position")
                                };
                                if vector(ui, label, &mut position) {
                                    self.edit(base + index, position, None, None);
                                }
                            }
                        }
                    }
                }
                ui.label(RichText::new(t!("propper.drag_collision")).weak());
            }
            2 => {
                ui.label(RichText::new(t!("propper.local_coordinates")).small().weak());
                let mut changed = ui.checkbox(&mut self.custom_illum, "$illumposition").changed();
                ui.add_enabled_ui(self.custom_illum, |ui| {
                    if self.custom_illum {
                        changed |= vector(ui, t!("propper.illum_position"), &mut self.illum);
                    } else {
                        vector(ui, t!("propper.illum_position"), &mut self.bounds.center());
                    }
                });
                if ui.button(t!("propper.select_illum")).clicked() {
                    self.custom_illum = true;
                    self.viewport.selected = Some(0);
                    changed = true;
                }
                ui.separator();
                changed |= ui.checkbox(&mut self.custom_bbox, t!("propper.custom_bbox")).changed();
                ui.add_enabled_ui(self.custom_bbox, |ui| {
                    if self.custom_bbox {
                        let mut center = self.bbox.center();
                        if vector(ui, t!("propper.bounds_center"), &mut center) {
                            let delta = center - self.bbox.center();
                            self.bbox.min += delta;
                            self.bbox.max += delta;
                            changed = true;
                        }
                        changed |= vector(ui, t!("propper.bounds_min"), &mut self.bbox.min);
                        changed |= vector(ui, t!("propper.bounds_max"), &mut self.bbox.max);
                    } else {
                        let mut bounds = self.bounds;
                        vector(ui, t!("propper.bounds_min"), &mut bounds.min);
                        vector(ui, t!("propper.bounds_max"), &mut bounds.max);
                    }
                });
                if ui.button(t!("propper.move_bounds")).clicked() {
                    self.custom_bbox = true;
                    self.viewport.selected = Some(9);
                    changed = true;
                }
                if ui.button(t!("propper.reset_bounds")).clicked() {
                    self.bbox = solid_bounds(self.bounds);
                    changed = true;
                }
                ui.label(RichText::new(t!("propper.drag_bounds")).weak());
                if changed {
                    self.revision += 1;
                }
            }
            _ => {
                ui.checkbox(&mut self.reuse_materials, t!("propper.reuse_materials"));
                ui.label(RichText::new(t!("propper.reuse_materials_tip")).small().weak());
                ui.separator();
                ui.label("$modelname").on_hover_text(t!("propper.modelname_tip"));
                self.model_field(ui, state);
                ui.label("$cdmaterials");
                ui.text_edit_singleline(&mut self.material_folder);
                ui.label(RichText::new(t!("propper.material_export_tip")).weak());
                ui.separator();
                ui.label("$surfaceprop");
                ui.text_edit_singleline(&mut self.surface);
                ui.text_edit_singleline(&mut self.surface_search);
                egui::ScrollArea::vertical().id_salt("propper_surfaces").max_height(220.0).show(ui, |ui| {
                    ui.selectable_value(&mut self.surface, String::new(), t!("propper.surface_auto"));
                    let search = self.surface_search.to_ascii_lowercase();
                    for surface in self.surfaces.iter().filter(|name| name.contains(&search)) {
                        ui.selectable_value(&mut self.surface, surface.clone(), surface);
                    }
                });
                ui.separator();
                ui.label(format!(
                    "{}: {}/materials/{}",
                    t!("propper.destination"),
                    self.destination,
                    self.directives().cdmaterials
                ));
            }
        }
        folder
    }

    fn edit(&mut self, key: usize, position: DVec3, start: Option<DVec3>, snap: Option<f64>) {
        if key == 0 {
            self.illum = position;
            self.custom_illum = true;
        } else if key <= 8 {
            let index = key - 1;
            for axis in 0..3 {
                if index & (1 << axis) == 0 {
                    self.bbox.min[axis] = position[axis].min(self.bbox.max[axis] - 0.001);
                } else {
                    self.bbox.max[axis] = position[axis].max(self.bbox.min[axis] + 0.001);
                }
            }
        } else if key == 9 {
            let delta = position - self.bbox.center();
            self.bbox.min += delta;
            self.bbox.max += delta;
        } else if let Some((element, index)) = key_element(key) {
            let Some(piece) = self.manual.as_ref().and_then(|pieces| pieces.get(self.piece)) else { return };
            let stamp = start.map(|s| [s.x.to_bits(), s.y.to_bits(), s.z.to_bits()]);
            let indices = match (&self.grab, stamp) {
                (Some((grab_key, grab_stamp, ids)), Some(stamp)) if *grab_key == key && *grab_stamp == stamp => {
                    ids.clone()
                }
                _ => match element_points(piece, element, index) {
                    Some(ids) => ids,
                    None => return,
                },
            };
            self.grab = stamp.map(|stamp| (key, stamp, indices.clone()));
            let Some(center) = piece.center_of(&indices) else { return };
            let target = match (snap, start) {
                (Some(grid), Some(_)) if element == CollisionElement::Vertex => snap_value(position, grid),
                (Some(grid), Some(start)) => start + snap_value(position - start, grid),
                _ => position,
            };
            let delta = target - center;
            if delta == DVec3::ZERO {
                return;
            }
            let piece = self.manual.as_mut().and_then(|pieces| pieces.get_mut(self.piece)).expect("checked above");
            if let Err(error) = piece.move_points(&indices, delta) {
                self.error = Some(edit_message(error));
                return;
            }
        }
        self.error = None;
        self.revision += 1;
    }

    fn points(&self) -> Vec<EditPoint> {
        let mut points = Vec::new();
        if self.custom_illum && self.show_illum {
            points.push(EditPoint { key: 0, position: self.illum, color: [1.0, 0.85, 0.1, 1.0] });
        }
        if self.custom_bbox && self.show_bbox {
            points.extend((0..8).map(|index| EditPoint {
                key: index + 1,
                position: corner(self.bbox, index),
                color: [0.1, 0.8, 1.0, 1.0],
            }));
            points.push(EditPoint { key: 9, position: self.bbox.center(), color: [0.1, 0.8, 1.0, 1.0] });
        }
        if self.collision == Collision::Manual
            && self.show_collision
            && let Some(piece) = self.manual_piece()
        {
            let base = element_key(self.element);
            let color = match self.element {
                CollisionElement::Vertex => [0.2, 1.0, 0.3, 1.0],
                CollisionElement::Edge => [1.0, 0.55, 0.1, 1.0],
                CollisionElement::Face => [0.3, 0.7, 1.0, 1.0],
            };
            for index in 0..element_count(piece, self.element) {
                if let Some(position) = element_points(piece, self.element, index).and_then(|ids| piece.center_of(&ids))
                {
                    points.push(EditPoint { key: base + index, position, color });
                }
            }
        }
        points
    }

    fn overlay(&mut self, collision: bool) -> Arc<Overlay> {
        let revision = self.revision * 2 + u64::from(collision);
        if let Some((old, overlay)) = &self.overlay
            && *old == revision
        {
            return Arc::clone(overlay);
        }
        let mut overlay = Overlay::default();
        if self.show_collision && collision {
            for piece in self.pieces() {
                for triangle in piece {
                    for index in 0..3 {
                        overlay.lines_on_top.push(OverlayLine {
                            a: triangle[index].as_vec3().to_array(),
                            b: triangle[(index + 1) % 3].as_vec3().to_array(),
                            color: [0.1, 1.0, 0.25, 0.9],
                        });
                    }
                }
            }
            if self.collision == Collision::Manual
                && let Some(piece) = self.manual_piece()
            {
                let selected = self.viewport.selected.and_then(key_element);
                let highlighted = selected.and_then(|(element, index)| element_points(piece, element, index));
                for edge in piece.edges() {
                    let hot = highlighted.as_ref().is_some_and(|ids| {
                        ids.contains(&edge[0]) && ids.contains(&edge[1]) && ids.len() <= 2
                            || selected.is_some_and(|(element, _)| element == CollisionElement::Face)
                                && ids.contains(&edge[0])
                                && ids.contains(&edge[1])
                    });
                    overlay.lines_on_top.push(OverlayLine {
                        a: piece.points[edge[0]].as_vec3().to_array(),
                        b: piece.points[edge[1]].as_vec3().to_array(),
                        color: if hot { [1.0, 1.0, 0.1, 1.0] } else { [0.6, 1.0, 0.6, 1.0] },
                    });
                }
            }
        }
        if self.show_bbox {
            let bounds = if self.custom_bbox { self.bbox } else { self.bounds };
            if bounds.min.is_finite() && bounds.max.is_finite() {
                for index in 0..8 {
                    for axis in 0..3 {
                        if index & (1 << axis) == 0 {
                            overlay.lines_on_top.push(OverlayLine {
                                a: corner(bounds, index).as_vec3().to_array(),
                                b: corner(bounds, index | (1 << axis)).as_vec3().to_array(),
                                color: [0.1, 0.7, 1.0, 0.8],
                            });
                        }
                    }
                }
            }
        }
        if self.show_illum {
            let position = if self.custom_illum { self.illum } else { self.bounds.center() };
            if position.is_finite() {
                overlay.points.push(OverlayPoint {
                    pos: position.as_vec3().to_array(),
                    color: [1.0, 0.85, 0.1, 1.0],
                    size_px: 10.0,
                });
            }
        }
        let overlay = Arc::new(overlay);
        self.overlay = Some((revision, Arc::clone(&overlay)));
        overlay
    }
}

impl MapDialog for CreateDialog {
    fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.lab.propper_create_open {
            self.draft = None;
            state.runtime.lab.propper_collision_info = None;
            state.runtime.lab.propper_collision_commands.clear();
            state.runtime.lab.propper_lod_camera = None;
            state.runtime.lab.propper_lod_camera_request = None;
            state.runtime.lab.propper_lod_levels.clear();
            return;
        }
        if let Some((folder, receiver)) = &self.picker {
            match receiver.try_recv() {
                Ok(path) => {
                    if let (Some(path), Some(draft)) = (path, &mut self.draft) {
                        match folder {
                            Folder::Destination => draft.destination = path.to_string_lossy().into_owned(),
                            Folder::Source => draft.source = path.to_string_lossy().into_owned(),
                        }
                    }
                    self.picker = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.picker = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.draft.is_none() {
            match Draft::new(state) {
                Ok(draft) => {
                    self.draft = Some(draft);
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
        let mut create = false;
        let mut cancel = false;
        let mut pick = None;
        let width = (ctx.content_rect().width() - 80.0).clamp(600.0, 1160.0);
        let height = (ctx.content_rect().height() - 200.0).clamp(280.0, 620.0);
        let modal = egui::Modal::new(egui::Id::unique("hp_propper_create")).show(ctx, |ui| {
            ui.set_width(width);
            ui.heading(t!("propper.create_title"));
            if let Some(draft) = &mut self.draft {
                draft.run_commands(state);
                ui.horizontal(|ui| {
                    for (index, key) in
                        ["propper.tab_model", "propper.tab_collision", "propper.tab_lighting", "propper.tab_materials"]
                            .into_iter()
                            .enumerate()
                    {
                        ui.selectable_value(&mut draft.tab, index, t!(key));
                    }
                });
                ui.separator();
                ui.horizontal_top(|ui| {
                    ui.set_height(height);
                    ui.allocate_ui_with_layout(
                        egui::vec2((width * 0.35).clamp(240.0, 350.0), height),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            egui::ScrollArea::vertical().id_salt("propper_creation_options").max_height(height).show(
                                ui,
                                |ui| {
                                    pick = draft.controls(ui, state);
                                },
                            );
                        },
                    );
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            let mut changed =
                                ui.checkbox(&mut draft.show_collision, t!("propper.preview_collision")).changed();
                            changed |= ui.checkbox(&mut draft.show_bbox, "$bbox").changed();
                            changed |= ui.checkbox(&mut draft.show_illum, "$illumposition").changed();
                            ui.checkbox(&mut draft.viewport.wireframe, t!("propper.preview_wire"));
                            if changed {
                                draft.revision += 1;
                            }
                            if ui.button(t!("propper.preview_fit")).clicked() {
                                let points = draft.points();
                                draft.viewport.fit(&draft.geometry, &points);
                            }
                            if state.settings.lab.propper_options().collision {
                                ui.label(RichText::new(tf!("propper.pieces_count", draft.pieces().len())).weak());
                            }
                        });
                        draft.lods.update(ui.ctx(), &draft.geometry, state.settings.lab.model_lod);
                        if let Some(distance) = state.runtime.lab.propper_lod_camera_request.take() {
                            draft.viewport.set_distance(distance);
                        }
                        if state.settings.lab.propper_lod_auto
                            && let Some(last) = draft.lods.levels().last()
                        {
                            draft.viewport.reach(last.game_distance() * 1.5);
                        }
                        let shown = draft.lods.bar(ui, state, draft.viewport.distance());
                        let display = Arc::clone(draft.lods.geometry(shown).unwrap_or(&draft.geometry));
                        let collision = state.settings.lab.propper_options().collision;
                        let overlay = draft.overlay(collision);
                        let mut points = draft.points();
                        if !collision {
                            points.retain(|point| point.key < 1000);
                        }
                        if let Some(edit) = draft.viewport.show(
                            ui,
                            state,
                            Scene {
                                geometry: &display,
                                overlay: &overlay,
                                points: &points,
                                revision: draft.revision * 2 + u64::from(collision),
                            },
                        ) {
                            let snap = (state.settings.lab.propper_collision_snap && edit.key >= VERTEX_KEY)
                                .then(|| state.grid_size());
                            draft.edit(edit.key, edit.position, Some(edit.start), snap);
                        }
                        ui.label(RichText::new(t!("propper.preview_controls")).small().weak());
                    });
                });
                draft.publish(state);
                let validation = draft.validation(state);
                if let Some(error) = &validation {
                    ui.colored_label(theme::warning(), error);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    create = ui.add_enabled(validation.is_none(), egui::Button::new(t!("propper.create"))).clicked();
                    cancel = ui.button(t!("common.cancel")).clicked();
                });
            } else {
                if let Some(error) = &self.error {
                    ui.colored_label(theme::warning(), error);
                }
                cancel = ui.button(t!("common.cancel")).clicked();
            }
        });
        #[cfg(test)]
        {
            self.last_rect = Some(modal.response.rect);
        }
        create |= !ctx.egui_wants_keyboard_input()
            && self.draft.as_ref().is_some_and(|draft| draft.validation(state).is_none())
            && enter_pressed(ctx, &modal.response);
        cancel |= modal.should_close();
        if let Some(folder) = pick {
            let (tx, rx) = mpsc::channel();
            let repaint = ctx.clone();
            std::thread::spawn(move || {
                let path = rfd::FileDialog::new().pick_folder();
                let _ = tx.send(path);
                repaint.request_repaint();
            });
            self.picker = Some((folder, rx));
        }
        if cancel || create {
            state.runtime.lab.propper_create_open = false;
            state.runtime.lab.propper_create_confirmed = create && !cancel;
            state.runtime.lab.propper_create_request =
                if create && !cancel { self.draft.as_ref().map(|draft| Box::new(draft.request(state))) } else { None };
            if create
                && !cancel
                && let Some(draft) = &self.draft
            {
                draft.remember(&mut state.settings.lab);
                state.settings_dirty = true;
            }
            self.draft = None;
            self.picker = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_keeps_the_edited_preview_and_rejects_stale_map_content() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let id = *state
            .doc
            .scene()
            .solids()
            .iter()
            .find(|(_, solid)| {
                solid.solid.sides.iter().any(|side| !side.material.to_ascii_lowercase().starts_with("tools/"))
            })
            .unwrap()
            .0;
        state.doc.select(&[hp_editor::ObjRef::Solid(id)], hp_editor::SelectOp::Replace);
        let mut draft = Draft::new(&mut state).unwrap();
        draft.model = "models/chosen/pillar.mdl".into();
        let root = std::env::temp_dir().join(format!("hp-prop-preview-{}", std::process::id()));
        draft.destination = root.join("export").to_string_lossy().into_owned();
        draft.source = root.join("source").to_string_lossy().into_owned();
        draft.material_folder = "materials/chosen/shared".into();
        draft.choose_collision(Collision::Manual, (16, 32));
        assert!(draft.error.is_none());
        let point = draft.manual.as_ref().unwrap()[0].points[0] + DVec3::new(-5.0, -3.0, -2.0);
        draft.edit(1000, point, None, None);
        assert!(draft.error.is_none());
        draft.edit(0, DVec3::new(1.25, -2.5, 30.0), None, None);
        draft.custom_bbox = true;
        draft.edit(1, draft.bbox.min - DVec3::splat(10.0), None, None);
        let size = draft.bbox.size();
        let center = draft.bbox.center() + DVec3::new(6.25, 3.0, -5.0);
        draft.edit(9, center, None, None);
        assert_eq!(draft.bbox.size(), size, "translating the center preserves dimensions");
        assert_eq!(draft.bbox.center(), center);
        state.settings.lab.propper_static = false;
        assert!(draft.validation(&state).is_none());
        let request = draft.request(&state);
        assert_eq!(request.model, "chosen/pillar");
        assert_eq!(request.solid_ids, [id]);
        assert_eq!(request.directives.illumposition, Some(draft.illum));
        assert_eq!(request.directives.bbox, Some(draft.bbox));
        assert_eq!(request.directives.cdmaterials, "chosen/shared");
        assert!(request.reuse_materials);
        assert_eq!(request.geometry.pieces[0], draft.manual.as_ref().unwrap()[0].triangles());
        assert_eq!(request.geometry.tris, draft.geometry.tris);
        assert!(!request.options.static_prop);
        assert!(!root.exists(), "preview and request assembly must not export files");
        state.doc.create_point_entity("info_target", DVec3::ZERO, &[]).unwrap();
        assert_eq!(draft.validation(&state), Some(t!("propper.source_changed").into()));
        for invalid in ["", "../escape", "models//a", "models/a b", "/absolute", "C:/absolute"] {
            assert!(model_path(invalid).is_none(), "{invalid}");
        }
    }

    #[test]
    fn decomposition_edge_and_face_moves_snap_and_stay_convex_and_commands_reach_the_window() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let ids: Vec<_> = state.doc.scene().solids().keys().copied().collect();
        let refs: Vec<_> = ids.iter().map(|id| hp_editor::ObjRef::Solid(*id)).collect();
        state.doc.select(&refs, hp_editor::SelectOp::Replace);
        state.settings.lab.propper_collision_mode = PropperCollisionMode::Decompose;
        state.settings.lab.collision_max_pieces = 4;
        state.settings.lab.collision_max_hull_vertices = 12;
        let mut draft = Draft::new(&mut state).unwrap();
        assert_eq!(draft.collision, Collision::Decompose, "the window starts in the saved precise mode");
        let pieces = draft.pieces();
        assert!(!pieces.is_empty() && pieces.len() <= 4, "{}", pieces.len());
        for piece in &draft.decomposed.as_ref().unwrap().1 {
            assert!(piece.points.len() <= 12);
        }
        draft.choose_collision(Collision::Manual, (4, 12));
        assert_eq!(draft.collision, Collision::Manual);
        let piece = draft.manual.as_ref().unwrap()[0].clone();
        let edge = piece.edges()[0];
        let start = piece.center_of(&edge).unwrap();
        draft.element = CollisionElement::Edge;
        let outward = (start - piece.bounds().center()).normalize() * 37.0;
        draft.edit(EDGE_KEY, start + outward, Some(start), Some(16.0));
        assert!(draft.error.is_none(), "{:?}", draft.error);
        let moved = &draft.manual.as_ref().unwrap()[0];
        let delta = moved.points[edge[0]] - piece.points[edge[0]];
        assert_eq!(delta, moved.points[edge[1]] - piece.points[edge[1]], "both vertices of the edge move");
        assert_eq!(delta, snap_value(outward, 16.0), "edges move by whole grid steps");
        let valid = draft.manual.as_ref().unwrap()[0].clone();
        let face = valid.faces()[0].clone();
        let center = valid.center_of(&face).unwrap();
        let inside = valid.bounds().center() * 2.0 - center;
        draft.edit(FACE_KEY, inside, None, None);
        assert_eq!(draft.error.as_deref(), Some(t!("propper.collision_folded")), "a fold is refused");
        assert_eq!(draft.manual.as_ref().unwrap()[0], valid, "the piece keeps its last convex shape");
        state.runtime.lab.propper_collision_commands = vec![
            PropperCollisionCommand::Move {
                piece: 0,
                element: CollisionElement::Vertex,
                index: 0,
                delta: DVec3::splat(-3.0),
                snap: true,
            },
            PropperCollisionCommand::ModelName("models/Props/Arch01.mdl".into()),
            PropperCollisionCommand::Mode("hull".into()),
        ];
        let before = draft.manual.as_ref().unwrap()[0].points[0];
        draft.run_commands(&mut state);
        assert!(state.runtime.lab.propper_collision_commands.is_empty());
        assert_eq!(
            draft.manual.as_ref().unwrap()[0].points[0],
            before + snap_value(DVec3::splat(-3.0), state.grid_size())
        );
        assert_eq!(draft.model, "models/Props/Arch01.mdl");
        assert_eq!(draft.collision, Collision::Hull);
        assert!(draft.pieces().len() == 1 && draft.hull.as_ref().unwrap().1.points.len() <= 12);
        draft.publish(&mut state);
        let info = state.runtime.lab.propper_collision_info.as_ref().unwrap();
        assert_eq!(info.mode, "hull");
        assert_eq!(info.modelname, "models/props/arch01.mdl");
        assert_eq!(info.pieces.len(), 1);
        state.settings.lab.propper_static = false;
        let request = draft.request(&state);
        assert_eq!(request.model, "props/arch01");
        let qc = request.geometry.files_with_options(&request.model, "", &[], request.options).qc;
        assert!(qc.contains("$modelname \"props/arch01.mdl\"") && !qc.contains("$concave"), "{qc}");
    }

    #[test]
    fn the_dialog_remembers_folders_and_options_and_offers_a_fresh_name() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let id = *state.doc.scene().solids().keys().min().unwrap();
        state.doc.select(&[hp_editor::ObjRef::Solid(id)], hp_editor::SelectOp::Replace);
        let first = Draft::new(&mut state).unwrap();
        assert!(first.model.starts_with("hammerpro/"), "first use keeps the generated name: {}", first.model);
        assert!(first.reuse_materials && first.surface.is_empty());
        let root = std::env::temp_dir().join(format!("hp-prop-remember-{}", std::process::id()));
        let export = root.join("export");
        std::fs::create_dir_all(export.join("models/custom/jedi_temple")).unwrap();
        std::fs::write(export.join("models/custom/jedi_temple/doorway02.mdl"), b"x").unwrap();
        let mut used = first;
        used.set_model("models/Custom/Jedi_Temple/doorway01.mdl".into());
        assert_eq!(used.material_folder, "models/custom/jedi_temple/doorway01", "materials follow the typed path");
        used.destination = export.to_string_lossy().into_owned();
        used.source = root.join("src").to_string_lossy().into_owned();
        used.reuse_materials = false;
        used.surface = used.surfaces.first().cloned().unwrap_or_default();
        used.remember(&mut state.settings.lab);
        let lab = &state.settings.lab;
        assert_eq!(lab.propper_model_folder, "custom/jedi_temple");
        assert_eq!(lab.propper_name_prefix, "doorway");
        assert_eq!(lab.propper_recent_folders, ["custom/jedi_temple"]);
        assert_eq!(lab.propper_material_dir, "", "a material folder that follows the model is not pinned");
        state
            .doc
            .create_point_entity(
                "prop_static",
                DVec3::ZERO,
                &[("model".to_owned(), "models/custom/jedi_temple/doorway01.mdl".to_owned())],
            )
            .unwrap();
        state.doc.select(&[hp_editor::ObjRef::Solid(id)], hp_editor::SelectOp::Replace);
        let next = Draft::new(&mut state).unwrap();
        assert_eq!(next.model, "custom/jedi_temple/doorway03", "skips names used in the map and on disk");
        assert_eq!(next.material_folder, "models/custom/jedi_temple/doorway03");
        assert_eq!(next.destination, used.destination);
        assert_eq!(next.source, used.source);
        assert!(!next.reuse_materials);
        assert_eq!(next.surface, used.surface);
        let mut other = next;
        state.settings.lab.remember_propper_folder("props/rocks");
        other.pick_folder("props/rocks", &state);
        assert_eq!(other.model, "props/rocks/doorway01");
        assert_eq!(other.material_folder, "models/props/rocks/doorway01");
        other.material_folder = "materials/shared".into();
        other.set_model("props/rocks/arch01".into());
        assert_eq!(other.material_folder, "materials/shared", "a custom material folder stays");
        other.remember(&mut state.settings.lab);
        assert_eq!(state.settings.lab.propper_material_dir, "materials/shared");
        assert_eq!(state.settings.lab.propper_recent_folders, ["props/rocks", "custom/jedi_temple"]);
        state.settings.lab.propper_export_dir = "relative/dir".into();
        let fallback = Draft::new(&mut state).unwrap();
        assert!(PathBuf::from(&fallback.destination).is_absolute(), "a relative saved folder is not reused");
        assert_eq!(fallback.material_folder, "materials/shared");
        assert_eq!(name_prefix("12"), "prop");
        assert_eq!(next_model("", "rock7", &|_| false), "rock01");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_different_document_with_the_same_history_serial_cannot_replace_the_source() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let id = *state.doc.scene().solids().keys().min().unwrap();
        state.doc.select(&[hp_editor::ObjRef::Solid(id)], hp_editor::SelectOp::Replace);
        let draft = Draft::new(&mut state).unwrap();
        let history = state.doc.content_serial();
        state.doc = hp_editor::Document::from_map(state.doc.to_map());
        assert_eq!(state.doc.content_serial(), history, "the history counter alone cannot identify the map");
        assert_eq!(draft.validation(&state), Some(t!("propper.source_changed").into()));
    }

    #[test]
    fn creation_tabs_fit_inside_desktop_and_laptop_windows() {
        let _lang = crate::lang_lock();
        crate::register_translations();
        for size in [egui::vec2(1920.0, 1080.0), egui::vec2(1600.0, 900.0), egui::vec2(1024.0, 768.0)] {
            let ctx = egui::Context::default();
            let mut state = hp_app::testing::test_state();
            let id = *state.doc.scene().solids().keys().min().unwrap();
            state.doc.select(&[hp_editor::ObjRef::Solid(id)], hp_editor::SelectOp::Replace);
            state.runtime.lab.propper_create_open = true;
            let mut dialog = CreateDialog::default();
            for tab in 0..4 {
                for _ in 0..4 {
                    if let Some(draft) = &mut dialog.draft {
                        draft.tab = tab;
                    }
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ui| dialog.show(&ui.ctx().clone(), &mut state),
                    );
                    output.textures_delta.clear();
                }
                let rect = dialog.last_rect.unwrap();
                assert!(
                    rect.min.x >= 0.0 && rect.min.y >= 0.0 && rect.max.x <= size.x && rect.max.y <= size.y,
                    "tab {tab}, window {size:?}, modal {rect:?}"
                );
            }
        }
    }
}
