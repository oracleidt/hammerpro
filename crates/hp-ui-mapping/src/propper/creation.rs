use std::path::PathBuf;
use std::sync::{Arc, mpsc};

use egui::{RichText, Ui};
use glam::DVec3;
use hp_app::runtime::PropCreationRequest;
use hp_app::{AppState, t, theme};
use hp_editor::prop_export::{EditableCollision, PropDirectives, PropGeometry, collect_prop};
use hp_geom::Aabb;
use hp_render::{Overlay, OverlayLine, OverlayPoint};

use super::viewport::{EditPoint, Scene, Viewport};
use crate::dialogs::MapDialog;
use crate::util::enter_pressed;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Collision {
    Brushes,
    Hull,
    Box,
    Manual,
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
    hull: Option<EditableCollision>,
    boxed: EditableCollision,
    piece: usize,
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

impl Draft {
    fn new(state: &mut AppState) -> Result<Self, String> {
        let mut ids = state.doc.selected_solid_ids();
        ids.sort_unstable();
        if ids.is_empty() {
            return Err(t!("propper.create_select").into());
        }
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
        let geometry = Arc::new(
            collect_prop(&state.doc, &ids, &|name| {
                sizes.get(&name.to_ascii_lowercase()).copied().unwrap_or((512, 512))
            })
            .ok_or_else(|| t!("lab.prop.empty").to_owned())?,
        );
        let bounds = Aabb::from_points(geometry.tris.iter().flat_map(|tri| tri.pos));
        let mut hash = 0x811c_9dc5u32;
        for byte in ids.iter().flat_map(|id| id.to_le_bytes()) {
            hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
        }
        let stem = state.doc.path().and_then(|path| path.file_stem()).map_or("map".into(), |s| s.to_string_lossy());
        let stem: String = stem.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(40).collect();
        let model = format!("hammerpro/{stem}_{hash:08x}");
        let destination = state
            .assets
            .fs()
            .and_then(|fs| fs.game_dir())
            .map(PathBuf::from)
            .or_else(|| state.settings.game.gmod_root.as_ref().map(|root| root.join("garrysmod")))
            .unwrap_or_else(|| hp_app::settings::Settings::cache_dir().join("prop_exports"));
        let source = hp_app::settings::Settings::cache_dir().join("propsrc");
        let surfaces = state.assets.fs().map_or_else(|| vec!["default".into()], |fs| fs.surface_properties());
        Ok(Self {
            // History serials are local to a map and can collide after a tab switch.
            // The global revision also identifies a newly loaded document safely.
            source_serial: state.doc.revision(),
            ids,
            geometry,
            material_folder: format!("models/{model}"),
            model,
            destination: destination.to_string_lossy().into_owned(),
            source: source.to_string_lossy().into_owned(),
            surface: String::new(),
            surfaces,
            surface_search: String::new(),
            reuse_materials: true,
            collision: Collision::Brushes,
            manual: None,
            hull: None,
            boxed: EditableCollision::box_at(solid_bounds(bounds)).map_err(|e| e.to_string())?,
            piece: 0,
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
            revision: 1,
            overlay: None,
            error: None,
        })
    }

    fn pieces(&self) -> Vec<&[[DVec3; 3]]> {
        match self.collision {
            Collision::Brushes => self.geometry.pieces.iter().map(Vec::as_slice).collect(),
            Collision::Hull => self.hull.iter().map(EditableCollision::triangles).collect(),
            Collision::Box => vec![self.boxed.triangles()],
            Collision::Manual => self.manual.iter().flatten().map(EditableCollision::triangles).collect(),
        }
    }

    fn choose_collision(&mut self, next: Collision) {
        let prepared = match next {
            Collision::Manual if self.manual.is_none() => self
                .pieces()
                .into_iter()
                .map(EditableCollision::from_triangles)
                .collect::<Result<Vec<_>, _>>()
                .map(|pieces| self.manual = Some(pieces)),
            Collision::Hull if self.hull.is_none() => {
                let mut points: Vec<DVec3> = self.geometry.pieces.iter().flatten().flatten().copied().collect();
                if points.is_empty() {
                    points.extend(self.geometry.tris.iter().flat_map(|tri| tri.pos));
                }
                EditableCollision::new(points).map(|hull| self.hull = Some(hull))
            }
            _ => Ok(()),
        };
        match prepared {
            Ok(()) => {
                self.collision = next;
                self.error = None;
                self.revision += 1;
                self.viewport.selected = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
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

    fn controls(&mut self, ui: &mut Ui, state: &mut AppState) -> Option<Folder> {
        let mut folder = None;
        match self.tab {
            0 => {
                ui.label(t!("propper.model_path"));
                ui.text_edit_singleline(&mut self.model);
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
                if level != previous {
                    settings.propper_collision = level != 0;
                    state.settings_dirty = true;
                    if level == 1 {
                        self.choose_collision(Collision::Box);
                    }
                    if level == 2 && self.collision == Collision::Box {
                        self.choose_collision(Collision::Brushes);
                    }
                }
                if level == 2 {
                    let old = self.collision;
                    let mut next = old;
                    ui.collapsing(t!("propper.collision_advanced"), |ui| {
                        for (mode, key) in [
                            (Collision::Brushes, "propper.collision_brushes"),
                            (Collision::Hull, "propper.collision_hull"),
                            (Collision::Manual, "propper.collision_manual"),
                        ] {
                            ui.radio_value(&mut next, mode, t!(key));
                        }
                    });
                    if next != old {
                        self.choose_collision(next);
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
                    if let Some(piece) = self.manual.as_ref().and_then(|pieces| pieces.get(self.piece)) {
                        let mut index = self
                            .viewport
                            .selected
                            .filter(|key| *key >= 1000)
                            .map_or(0, |key| key - 1000)
                            .min(piece.points.len().saturating_sub(1));
                        egui::ComboBox::from_id_salt("propper_collision_vertex")
                            .selected_text(format!("{} {}", t!("propper.vertex"), index + 1))
                            .show_ui(ui, |ui| {
                                for point in 0..piece.points.len() {
                                    ui.selectable_value(&mut index, point, (point + 1).to_string());
                                }
                            });
                        self.viewport.selected = Some(1000 + index);
                        let mut position = piece.points[index];
                        if vector(ui, t!("propper.vertex_position"), &mut position) {
                            self.edit(1000 + index, position);
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

    fn edit(&mut self, key: usize, position: DVec3) {
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
        } else if key >= 1000
            && let Some(piece) = self.manual.as_mut().and_then(|pieces| pieces.get_mut(self.piece))
            && let Err(error) = piece.set_point(key - 1000, position)
        {
            self.error = Some(error.to_string());
            return;
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
            && let Some(piece) = self.manual.as_ref().and_then(|pieces| pieces.get(self.piece))
        {
            points.extend(piece.points.iter().enumerate().map(|(index, point)| EditPoint {
                key: 1000 + index,
                position: *point,
                color: [0.2, 1.0, 0.3, 1.0],
            }));
        }
        points
    }

    fn overlay(&mut self, collision: bool) -> Arc<Overlay> {
        // The collision checkbox also affects the preview, even when changed on another tab.
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
                        });
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
                                geometry: &draft.geometry,
                                overlay: &overlay,
                                points: &points,
                                revision: draft.revision * 2 + u64::from(collision),
                            },
                        ) {
                            draft.edit(edit.key, edit.position);
                        }
                        ui.label(RichText::new(t!("propper.preview_controls")).small().weak());
                    });
                });
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
        draft.choose_collision(Collision::Manual);
        assert!(draft.error.is_none());
        let point = draft.manual.as_ref().unwrap()[0].points[0] + DVec3::new(-5.0, -3.0, -2.0);
        draft.edit(1000, point);
        assert!(draft.error.is_none());
        draft.edit(0, DVec3::new(1.25, -2.5, 30.0));
        draft.custom_bbox = true;
        draft.edit(1, draft.bbox.min - DVec3::splat(10.0));
        let size = draft.bbox.size();
        let center = draft.bbox.center() + DVec3::new(6.25, 3.0, -5.0);
        draft.edit(9, center);
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
