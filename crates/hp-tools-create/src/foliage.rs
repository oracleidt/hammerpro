use std::collections::HashSet;

use egui::{Align2, Color32, CursorIcon, Modifiers, Pos2, RichText, Ui};
use glam::DVec3;
use hp_app::dnd::DragPayload;
use hp_app::foliage::{
    DENSITY_RANGE, FoliageMode, FoliagePalette, FoliageSettings, FoliageStroke, MAX_PALETTES, RADIUS_RANGE,
    apply_erase, apply_paint, is_paint_surface, surface_below,
};
use hp_app::foliage::{DETAIL_OVERRIDE_KEY as OVERRIDE_KEY, DETAIL_OVERRIDE_NONE as OVERRIDE_NONE};
use hp_app::i18n::TrTable;
use hp_app::state::AppState;
use hp_app::tool::ToolOption;
use hp_app::{Action, Severity, Tool, ToolFlow, ToolId, ViewInfo, ViewInput, ViewKind, t, tf};
use hp_editor::{FaceRef, ObjRef, PickOptions};
use hp_geom::{Falloff, PaintBrush, PaintMode, Ray};
use hp_render::Overlay;

use crate::common::{Mouse, Ov, fresh, on_off, option_flag, paint_label, rgba};

const CIRCLE_SEGMENTS: usize = 64;
const WHEEL_STEP: f64 = 1.15;
const COLOR_PAINT: Color32 = Color32::from_rgb(110, 220, 120);
const COLOR_ERASE: Color32 = Color32::from_rgb(255, 110, 100);
const COLOR_DETAIL: Color32 = Color32::from_rgb(200, 230, 90);
const CLASSES: [&str; 3] = ["prop_static", "prop_dynamic", "prop_physics"];

pub const FOLIAGE_TR: TrTable = &[
    ("foliage.mode.paint", "Paint", "Peindre"),
    ("foliage.mode.erase", "Erase", "Effacer"),
    ("foliage.mode.detail_alpha", "Detail alpha", "Alpha des détails"),
    ("foliage.mode.detail_face", "Detail per face", "Détails par face"),
    (
        "foliage.mode.paint_tip",
        "Paint props of the palette on displacements, brushes and other props (Shift erases)",
        "Peindre les props de la palette sur les déplacements, les brushes et les autres props (Maj efface)",
    ),
    (
        "foliage.mode.erase_tip",
        "Remove the props of this palette's layer under the brush",
        "Retirer les props du calque de cette palette sous le pinceau",
    ),
    (
        "foliage.mode.detail_alpha_tip",
        "Paint the displacement alpha that chooses where the detail props (grass) grow (Shift lowers it)",
        "Peindre l'alpha des déplacements qui choisit où poussent les detail props (herbe) (Maj le baisse)",
    ),
    (
        "foliage.mode.detail_face_tip",
        "Give the faces under the cursor their own detail type, or none to keep them bare (Shift clears it)",
        "Donner aux faces sous le curseur leur propre type de détail, ou none pour les laisser nues (Maj l'efface)",
    ),
    ("foliage.palette", "Palette:", "Palette :"),
    ("foliage.palette_new", "New palette", "Nouvelle palette"),
    ("foliage.palette_delete", "Delete this palette", "Supprimer cette palette"),
    (
        "foliage.palette_name",
        "Name of the palette, also the name of its visgroup layer",
        "Nom de la palette, aussi celui de son calque visgroup",
    ),
    (
        "foliage.drop",
        "Drop models here from the model browser",
        "Déposez ici des modèles depuis le navigateur de modèles",
    ),
    ("foliage.add_current", "Add the current model", "Ajouter le modèle courant"),
    (
        "foliage.add_current_tip",
        "Add the model chosen in the model browser",
        "Ajouter le modèle choisi dans le navigateur de modèles",
    ),
    ("foliage.empty", "The palette is empty: drop models in it", "La palette est vide : déposez-y des modèles"),
    ("foliage.weight", "Weight", "Poids"),
    ("foliage.remove", "Remove from the palette", "Retirer de la palette"),
    ("foliage.radius", "Radius:", "Rayon :"),
    ("foliage.radius_tip", "Brush radius (Alt + mouse wheel)", "Rayon du pinceau (Alt + molette)"),
    ("foliage.density", "Density:", "Densité :"),
    ("foliage.density_tip", "Props per 256 x 256 units of surface", "Props par 256 x 256 unités de surface"),
    ("foliage.spacing", "Spacing:", "Espacement :"),
    (
        "foliage.spacing_tip",
        "Least distance between two props of the layer",
        "Distance minimale entre deux props du calque",
    ),
    ("foliage.scale", "Scale:", "Échelle :"),
    (
        "foliage.scale_tip",
        "Random scale range, written to modelscale",
        "Plage d'échelle aléatoire, écrite dans modelscale",
    ),
    ("foliage.random_yaw", "Random yaw", "Lacet aléatoire"),
    ("foliage.align", "Align:", "Alignement :"),
    (
        "foliage.align_tip",
        "How much props lean to the surface normal (0 upright, 1 fully)",
        "À quel point les props suivent la normale (0 droits, 1 complètement)",
    ),
    ("foliage.slope", "Slope:", "Pente :"),
    (
        "foliage.slope_tip",
        "Only on surfaces this steep, in degrees",
        "Seulement sur les surfaces de cette pente, en degrés",
    ),
    ("foliage.height", "Height limit", "Limite de hauteur"),
    ("foliage.height_tip", "Only between these heights (world Z)", "Seulement entre ces hauteurs (Z du monde)"),
    ("foliage.avoid_overlap", "Avoid overlap", "Éviter les chevauchements"),
    (
        "foliage.avoid_overlap_tip",
        "Keep the real model footprints apart, painted and existing props alike",
        "Garder les empreintes réelles des modèles séparées, props peints comme existants",
    ),
    ("foliage.drop_to_surface", "Drop to surface", "Poser sur la surface"),
    (
        "foliage.drop_to_surface_tip",
        "Rest the lowest point of the real mesh on the surface, never floating",
        "Poser le point le plus bas du maillage réel sur la surface, jamais en l'air",
    ),
    ("foliage.random_skin", "Random skin", "Skin aléatoire"),
    ("foliage.on_props", "Paint on props", "Peindre sur les props"),
    (
        "foliage.on_props_tip",
        "Props of other layers and the map receive props too (grass on rocks)",
        "Les props des autres calques et de la carte en reçoivent aussi (herbe sur rochers)",
    ),
    ("foliage.sink", "Sink:", "Enfoncer :"),
    (
        "foliage.sink_tip",
        "Offset along the up direction, negative sinks the props",
        "Décalage vers le haut, négatif enfonce les props",
    ),
    ("foliage.strength", "Strength:", "Force :"),
    ("foliage.strength_tip", "Alpha added per dab (0 to 255)", "Alpha ajouté par touche (0 à 255)"),
    ("foliage.detail_type", "Detail type:", "Type de détail :"),
    (
        "foliage.detail_type_tip",
        "A group of detail.vbsp, none for no detail props, empty to use the material",
        "Un groupe de detail.vbsp, none pour aucun detail prop, vide pour celui du matériau",
    ),
    ("foliage.show_preview", "Show the detail preview", "Afficher l'aperçu des détails"),
    (
        "foliage.hint",
        "{0}: drag to paint, Shift + drag to erase, Alt + wheel for the radius",
        "{0} : glissez pour peindre, Maj + glisser pour effacer, Alt + molette pour le rayon",
    ),
    (
        "foliage.hint_detail",
        "Drag to paint where the detail props grow, Shift + drag to remove them",
        "Glissez pour peindre où poussent les detail props, Maj + glisser pour les retirer",
    ),
    (
        "foliage.hint_empty",
        "Drop models from the model browser in the palette first",
        "Déposez d'abord des modèles du navigateur dans la palette",
    ),
    ("foliage.failed", "Foliage paint failed: {0}", "Échec de la peinture de végétation : {0}"),
    ("foliage.placed", "{0} props painted in {1}", "{0} props peints dans {1}"),
    ("foliage.erased", "{0} props erased", "{0} props effacés"),
];

fn mode_keys(mode: FoliageMode) -> (&'static str, &'static str) {
    match mode {
        FoliageMode::Paint => ("foliage.mode.paint", "foliage.mode.paint_tip"),
        FoliageMode::Erase => ("foliage.mode.erase", "foliage.mode.erase_tip"),
        FoliageMode::DetailAlpha => ("foliage.mode.detail_alpha", "foliage.mode.detail_alpha_tip"),
        FoliageMode::DetailFace => ("foliage.mode.detail_face", "foliage.mode.detail_face_tip"),
    }
}

#[derive(Clone, Copy, Debug)]
struct Hover {
    view: usize,
    frame: u64,
    center: DVec3,
    normal: DVec3,
    radius: f64,
    erase: bool,
}

enum Work {
    Foliage(Box<FoliageStroke>),
    Alpha { amount: f64 },
    Faces { clear: bool, done: HashSet<FaceRef> },
}

struct Active {
    work: Work,
    view: usize,
    last: Option<Pos2>,
    changed: usize,
}

pub struct FoliageTool {
    hover: Option<Hover>,
    active: Option<Active>,
    seed: u64,
    options: Vec<ToolOption>,
    pub last_report: Option<String>,
}

impl Default for FoliageTool {
    fn default() -> Self {
        Self::new()
    }
}

fn mode_effective(mode: FoliageMode, shift: bool) -> (FoliageMode, bool) {
    match (mode, shift) {
        (FoliageMode::Paint, true) => (FoliageMode::Erase, true),
        (FoliageMode::Erase, _) => (FoliageMode::Erase, true),
        (m, inverted) => (m, inverted),
    }
}

pub fn set_face_override(state: &mut AppState, face: FaceRef, value: Option<&str>) -> Result<bool, String> {
    let value = value.map(str::trim).filter(|v| !v.is_empty()).map(str::to_owned);
    state
        .doc
        .edit_solid(face.solid, |s| {
            let Some(side) = s.sides.get_mut(face.side) else { return };
            side.extra_props.retain(|(k, _)| !k.eq_ignore_ascii_case(OVERRIDE_KEY));
            if let Some(v) = value {
                side.extra_props.push((OVERRIDE_KEY.to_owned(), v));
            }
        })
        .map_err(|e| e.to_string())
}

pub fn paint_detail_alpha(state: &mut AppState, center: DVec3, radius: f64, amount: f64) -> Result<usize, String> {
    let brush =
        PaintBrush { center, radius, amount, mode: PaintMode::AlphaPaint, falloff: Falloff::Smooth, spatial: true };
    state.doc.disp_paint(&[], &brush).map_err(|e| e.to_string())
}

impl FoliageTool {
    pub fn new() -> Self {
        Self {
            hover: None,
            active: None,
            seed: 0x0F01_1A6E_0000_0001,
            options: foliage_options(&FoliageSettings::default()),
            last_report: None,
        }
    }

    pub fn is_painting(&self) -> bool {
        self.active.is_some()
    }

    fn brush_at(state: &AppState, view: &ViewInfo, pos: Pos2) -> Option<(DVec3, DVec3, Option<FaceRef>)> {
        if view.is_2d() {
            if view.kind != ViewKind::Top {
                return None;
            }
            let p = view.kind.from_2d(view.screen_to_2d(pos), 0.0);
            let top = state.doc.scene().solids().values().map(|n| n.bounds.max.z).fold(0.0, f64::max) + 64.0;
            let hit = surface_below(state, DVec3::new(p.x, p.y, top), 1.0)?;
            return Some((hit.point, hit.normal, None));
        }
        let (o, d) = view.ray(pos);
        if !o.is_finite() || !d.is_finite() {
            return None;
        }
        let scene = state.doc.scene();
        let filter = |obj: ObjRef| match obj {
            ObjRef::Solid(_) => is_paint_surface(scene, obj),
            ObjRef::Entity(id) => scene.entity(id).is_some_and(|n| hp_app::foliage::is_prop_entity(&n.entity)),
        };
        let face_hidden = |m: &str| hp_app::foliage::is_tool_material(m);
        let opts = PickOptions { filter: Some(&filter), face_hidden: Some(&face_hidden), ..PickOptions::default() };
        let hit = state.view_pick(&Ray::new(o, d), &opts)?;
        let n = hit.normal.try_normalize().filter(|n| n.is_finite()).unwrap_or(DVec3::Z);
        let face = match (hit.obj, hit.face) {
            (ObjRef::Solid(id), Some(side)) => Some(FaceRef::new(id, side)),
            _ => None,
        };
        hit.point.is_finite().then_some((hit.point, n, face))
    }

    fn begin(&mut self, state: &mut AppState, view: usize, shift: bool) -> bool {
        let settings = state.settings.placement.foliage.clone();
        let (mode, inverted) = mode_effective(settings.mode, shift);
        self.seed = self.seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        let work = match mode {
            FoliageMode::Paint | FoliageMode::Erase => {
                let erase = mode == FoliageMode::Erase;
                if !erase && settings.palette().is_none_or(|p| !p.items.iter().any(|i| i.usable())) {
                    state.notify(Severity::Info, t!("foliage.hint_empty"));
                    return false;
                }
                state.doc.begin(if erase { "Foliage erase" } else { "Foliage paint" });
                Work::Foliage(Box::new(FoliageStroke::begin(state, &settings, erase, self.seed)))
            }
            FoliageMode::DetailAlpha => {
                state.doc.begin("Detail paint");
                let a = settings.detail_strength.clamp(0.0, 255.0);
                Work::Alpha { amount: if inverted { -a } else { a } }
            }
            FoliageMode::DetailFace => {
                state.doc.begin("Detail paint");
                Work::Faces { clear: inverted, done: HashSet::new() }
            }
        };
        self.active = Some(Active { work, view, last: None, changed: 0 });
        true
    }

    fn end(&mut self, state: &mut AppState) {
        let Some(a) = self.active.take() else { return };
        state.doc.commit();
        if let Work::Foliage(stroke) = &a.work {
            let layer = stroke.palette.layer_name();
            self.last_report = Some(if stroke.erase {
                tf!("foliage.erased", stroke.removed_ids.len())
            } else {
                tf!("foliage.placed", stroke.placed_ids.len(), layer)
            });
        }
    }

    fn dab(&mut self, state: &mut AppState, center: DVec3, normal: DVec3, face: Option<FaceRef>) {
        let Some(active) = &mut self.active else { return };
        let radius = state.settings.placement.foliage.radius();
        let result = match &mut active.work {
            Work::Foliage(stroke) => {
                if stroke.erase {
                    apply_erase(state, stroke, center, normal)
                } else {
                    apply_paint(state, stroke, center, normal)
                }
            }
            Work::Alpha { amount } => paint_detail_alpha(state, center, radius, *amount),
            Work::Faces { clear, done } => match face {
                Some(f) if done.insert(f) => {
                    let ty = state.settings.placement.foliage.detail_type.clone();
                    let value = if *clear {
                        None
                    } else {
                        Some(if ty.trim().is_empty() { OVERRIDE_NONE } else { ty.as_str() })
                    };
                    set_face_override(state, f, value).map(usize::from)
                }
                _ => Ok(0),
            },
        };
        match result {
            Ok(n) => active.changed += n,
            Err(e) => state.notify(Severity::Error, tf!("foliage.failed", e)),
        }
    }

    fn paint_to(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2) {
        let Some(last) = self.active.as_ref().map(|a| a.last) else { return };
        let points: Vec<Pos2> = match last {
            Some(prev) => {
                let r = state.settings.placement.foliage.radius();
                let px = (r / view.units_per_point().max(1e-6)) as f32;
                let step = (px * 0.35).max(3.0);
                let n = ((prev.distance(pos) / step).ceil() as usize).clamp(1, 32);
                (1..=n).map(|i| prev.lerp(pos, i as f32 / n as f32)).collect()
            }
            None => vec![pos],
        };
        for p in points {
            if let Some((c, n, face)) = Self::brush_at(state, view, p) {
                self.dab(state, c, n, face);
            }
        }
        if let Some(a) = &mut self.active {
            a.last = Some(pos);
        }
    }

    pub(crate) fn handle_mouse(&mut self, state: &mut AppState, view: &ViewInfo, m: &Mouse) -> ToolFlow {
        let Some(pos) = m.pos else {
            self.hover = None;
            return ToolFlow::Ignored;
        };
        let shift = m.modifiers.shift;
        let settings = &state.settings.placement.foliage;
        let (mode, inverted) = mode_effective(settings.mode, shift);
        let radius = settings.radius();
        self.hover = Self::brush_at(state, view, pos).map(|(center, normal, _)| Hover {
            view: view.index,
            frame: state.frame.index,
            center,
            normal,
            radius,
            erase: inverted || mode == FoliageMode::Erase,
        });
        if m.drag_started || (m.clicked && self.active.is_none()) {
            self.end(state);
            if self.begin(state, view.index, shift) {
                self.paint_to(state, view, m.origin().unwrap_or(pos));
                self.paint_to(state, view, pos);
            }
        } else if m.dragging && self.active.as_ref().is_some_and(|a| a.view == view.index) {
            self.paint_to(state, view, pos);
        }
        if m.clicked || m.drag_stopped {
            self.end(state);
        }
        if m.any_button() { ToolFlow::Handled } else { ToolFlow::Ignored }
    }

    fn palette_ui(ui: &mut Ui, state: &mut AppState) -> bool {
        let mut changed = false;
        let f = &mut state.settings.placement.foliage;
        if f.palettes.is_empty() {
            f.palettes.push(FoliagePalette::default());
            changed = true;
        }
        if f.active >= f.palettes.len() {
            f.active = 0;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("foliage.palette"));
            let current = f.palettes[f.active].name.clone();
            egui::ComboBox::from_id_salt("hp_foliage_palette").selected_text(current).width(130.0).show_ui(ui, |ui| {
                for i in 0..f.palettes.len() {
                    if ui.selectable_label(i == f.active, f.palettes[i].name.as_str()).clicked() && i != f.active {
                        f.active = i;
                        changed = true;
                    }
                }
            });
            if ui
                .add_enabled(f.palettes.len() < MAX_PALETTES, egui::Button::new("➕").small())
                .on_hover_text(t!("foliage.palette_new"))
                .clicked()
            {
                let mut k = f.palettes.len() + 1;
                while f.palettes.iter().any(|p| p.name == format!("Foliage {k}")) {
                    k += 1;
                }
                f.palettes.push(FoliagePalette { name: format!("Foliage {k}"), items: Vec::new() });
                f.active = f.palettes.len() - 1;
                changed = true;
            }
            if ui
                .add_enabled(f.palettes.len() > 1, egui::Button::new("🗑").small())
                .on_hover_text(t!("foliage.palette_delete"))
                .clicked()
            {
                f.palettes.remove(f.active);
                f.active = f.active.min(f.palettes.len().saturating_sub(1));
                changed = true;
            }
        });
        let p = f.palette_mut();
        changed |= ui
            .add(egui::TextEdit::singleline(&mut p.name).desired_width(ui.available_width().min(200.0)))
            .on_hover_text(t!("foliage.palette_name"))
            .changed();
        let mut remove = None;
        if p.items.is_empty() {
            ui.label(RichText::new(t!("foliage.empty")).weak().italics());
        }
        for (i, item) in p.items.iter_mut().enumerate() {
            ui.horizontal_wrapped(|ui| {
                changed |= ui.checkbox(&mut item.enabled, "").changed();
                let short = item.model.rsplit('/').next().unwrap_or(&item.model).to_owned();
                ui.label(short).on_hover_text(item.model.as_str());
                changed |= ui
                    .add(egui::DragValue::new(&mut item.weight).range(0.0..=1000.0).speed(0.05).max_decimals(2))
                    .on_hover_text(t!("foliage.weight"))
                    .changed();
                egui::ComboBox::from_id_salt(("hp_foliage_class", i))
                    .selected_text(item.classname.as_str())
                    .width(96.0)
                    .show_ui(ui, |ui| {
                        for c in CLASSES {
                            if ui.selectable_label(item.classname == c, c).clicked() && item.classname != c {
                                item.classname = c.to_owned();
                                changed = true;
                            }
                        }
                    });
                if ui.small_button("✖").on_hover_text(t!("foliage.remove")).clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            p.items.remove(i);
            changed = true;
        }
        let frame = egui::Frame::group(ui.style());
        let (_, dropped) = ui.dnd_drop_zone::<DragPayload, _>(frame, |ui| {
            ui.label(RichText::new(t!("foliage.drop")).weak());
        });
        if let Some(payload) = dropped
            && let DragPayload::Model { path, .. } = payload.as_ref()
        {
            changed |= state.settings.placement.foliage.palette_mut().add_model(path);
        }
        let current = state.current_model.clone();
        if ui
            .add_enabled(current.is_some(), egui::Button::new(t!("foliage.add_current")))
            .on_hover_text(t!("foliage.add_current_tip"))
            .clicked()
            && let Some(m) = current
        {
            changed |= state.settings.placement.foliage.palette_mut().add_model(&m);
        }
        changed
    }

    fn brush_ui(ui: &mut Ui, state: &mut AppState) -> bool {
        let mut changed = false;
        let f = &mut state.settings.placement.foliage;
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("foliage.radius"));
            changed |= ui
                .add(egui::DragValue::new(&mut f.radius).range(RADIUS_RANGE).speed(2.0))
                .on_hover_text(t!("foliage.radius_tip"))
                .changed();
        });
        match f.mode {
            FoliageMode::Paint | FoliageMode::Erase => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("foliage.density"));
                    changed |= ui
                        .add(egui::DragValue::new(&mut f.density).range(DENSITY_RANGE).speed(0.1).max_decimals(2))
                        .on_hover_text(t!("foliage.density_tip"))
                        .changed();
                    ui.label(t!("foliage.spacing"));
                    changed |= ui
                        .add(egui::DragValue::new(&mut f.spacing).range(0.0..=4096.0).speed(1.0))
                        .on_hover_text(t!("foliage.spacing_tip"))
                        .changed();
                });
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("foliage.scale")).on_hover_text(t!("foliage.scale_tip"));
                    changed |= ui.add(egui::DragValue::new(&mut f.scale_min).range(0.01..=100.0).speed(0.01)).changed();
                    changed |= ui.add(egui::DragValue::new(&mut f.scale_max).range(0.01..=100.0).speed(0.01)).changed();
                });
                let (label, moved) = hp_app::widgets::slider_row(ui, t!("foliage.align"), |ui| {
                    ui.add(egui::Slider::new(&mut f.align, 0.0..=1.0)).changed()
                });
                label.on_hover_text(t!("foliage.align_tip"));
                changed |= moved;
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("foliage.slope")).on_hover_text(t!("foliage.slope_tip"));
                    changed |= ui.add(egui::DragValue::new(&mut f.slope_min).range(0.0..=180.0).suffix("°")).changed();
                    changed |= ui.add(egui::DragValue::new(&mut f.slope_max).range(0.0..=180.0).suffix("°")).changed();
                });
                ui.horizontal_wrapped(|ui| {
                    changed |= ui
                        .checkbox(&mut f.height_limit, t!("foliage.height"))
                        .on_hover_text(t!("foliage.height_tip"))
                        .changed();
                    let on = f.height_limit;
                    changed |= ui.add_enabled(on, egui::DragValue::new(&mut f.height_min).speed(4.0)).changed();
                    changed |= ui.add_enabled(on, egui::DragValue::new(&mut f.height_max).speed(4.0)).changed();
                });
                changed |= ui.checkbox(&mut f.random_yaw, t!("foliage.random_yaw")).changed();
                changed |= ui.checkbox(&mut f.random_skin, t!("foliage.random_skin")).changed();
                changed |= ui
                    .checkbox(&mut f.avoid_overlap, t!("foliage.avoid_overlap"))
                    .on_hover_text(t!("foliage.avoid_overlap_tip"))
                    .changed();
                changed |= ui
                    .checkbox(&mut f.drop_to_surface, t!("foliage.drop_to_surface"))
                    .on_hover_text(t!("foliage.drop_to_surface_tip"))
                    .changed();
                changed |= ui
                    .checkbox(&mut f.on_props, t!("foliage.on_props"))
                    .on_hover_text(t!("foliage.on_props_tip"))
                    .changed();
                ui.horizontal_wrapped(|ui| {
                    ui.label(t!("foliage.sink")).on_hover_text(t!("foliage.sink_tip"));
                    changed |= ui.add(egui::DragValue::new(&mut f.sink).range(-4096.0..=4096.0).speed(0.5)).changed();
                });
            }
            FoliageMode::DetailAlpha | FoliageMode::DetailFace => {
                if f.mode == FoliageMode::DetailAlpha {
                    let (_, moved) = hp_app::widgets::slider_row(ui, t!("foliage.strength"), |ui| {
                        ui.add(egui::Slider::new(&mut f.detail_strength, 1.0..=255.0).integer())
                            .on_hover_text(t!("foliage.strength_tip"))
                            .changed()
                    });
                    changed |= moved;
                } else {
                    let names = state.runtime.render.detail_preview.type_names.clone();
                    let f = &mut state.settings.placement.foliage;
                    ui.horizontal_wrapped(|ui| {
                        ui.label(t!("foliage.detail_type")).on_hover_text(t!("foliage.detail_type_tip"));
                        changed |=
                            ui.add(egui::TextEdit::singleline(&mut f.detail_type).desired_width(110.0)).changed();
                        egui::ComboBox::from_id_salt("hp_foliage_detail_type").selected_text("").width(24.0).show_ui(
                            ui,
                            |ui| {
                                for n in std::iter::once(OVERRIDE_NONE.to_owned()).chain(names) {
                                    if ui.selectable_label(f.detail_type == n, n.as_str()).clicked() {
                                        f.detail_type = n;
                                        changed = true;
                                    }
                                }
                            },
                        );
                    });
                }
                if !state.settings.render.detail_preview && ui.button(t!("foliage.show_preview")).clicked() {
                    state.request(Action::ToggleDetailPreview);
                }
            }
        }
        changed
    }
}

fn circle(center: DVec3, normal: DVec3, radius: f64) -> Vec<DVec3> {
    let n = normal.try_normalize().unwrap_or(DVec3::Z);
    let t = if n.z.abs() < 0.9 { DVec3::Z.cross(n) } else { DVec3::X.cross(n) };
    let t = t.try_normalize().unwrap_or(DVec3::X);
    let b = n.cross(t);
    (0..CIRCLE_SEGMENTS)
        .map(|i| {
            let a = i as f64 / CIRCLE_SEGMENTS as f64 * std::f64::consts::TAU;
            center + n * 0.5 + (t * a.cos() + b * a.sin()) * radius
        })
        .collect()
}

impl Tool for FoliageTool {
    fn id(&self) -> ToolId {
        ToolId::Foliage
    }

    fn deactivate(&mut self, state: &mut AppState) {
        self.end(state);
        self.hover = None;
    }

    fn view_input(&mut self, state: &mut AppState, input: &ViewInput<'_>) -> ToolFlow {
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
        let f = &mut state.settings.placement.foliage;
        let r = f.radius() * WHEEL_STEP.powf(f64::from(notches.clamp(-10.0, 10.0)));
        f.radius = r.clamp(*RADIUS_RANGE.start(), *RADIUS_RANGE.end()).round();
        state.settings_dirty = true;
        ToolFlow::Handled
    }

    fn key_input(&mut self, state: &mut AppState, ctx: &egui::Context) -> bool {
        if self.active.is_some() && !ctx.input(|i| i.pointer.primary_down()) {
            self.end(state);
        }
        false
    }

    fn overlay(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let Some(h) = self.hover.filter(|h| fresh(state, h.frame)) else { return };
        let mode = state.settings.placement.foliage.mode;
        let color = if h.erase {
            COLOR_ERASE
        } else if matches!(mode, FoliageMode::DetailAlpha | FoliageMode::DetailFace) {
            COLOR_DETAIL
        } else {
            COLOR_PAINT
        };
        let mut ov = Ov::new(out, view);
        ov.polygon_outline(&circle(h.center, h.normal, h.radius), rgba(color, 0.95), true);
        ov.xray_line(h.center, h.center + h.normal * h.radius.min(64.0) * 0.5, rgba(color, 0.9));
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let Some(h) = self.hover.filter(|h| fresh(state, h.frame) && h.view == view.index) else { return };
        let Some(s) = view.world_to_screen(h.center) else { return };
        if !view.rect.contains(s) {
            return;
        }
        let f = &state.settings.placement.foliage;
        let label = match f.mode {
            FoliageMode::Paint | FoliageMode::Erase => f.palette().map(|p| p.name.clone()).unwrap_or_default(),
            m => t!(mode_keys(m).0).to_owned(),
        };
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let color = if h.erase { COLOR_ERASE } else { COLOR_PAINT };
        paint_label(&painter, s + egui::vec2(12.0, -12.0), Align2::LEFT_BOTTOM, &label, color);
    }

    fn wants_side_panel(&self) -> bool {
        true
    }

    fn options_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let mut changed = false;
        ui.horizontal_wrapped(|ui| {
            for mode in FoliageMode::ALL {
                let (label, tip) = mode_keys(mode);
                let f = &mut state.settings.placement.foliage;
                if ui.selectable_label(f.mode == mode, t!(label)).on_hover_text(t!(tip)).clicked() && f.mode != mode {
                    f.mode = mode;
                    changed = true;
                }
            }
        });
        ui.separator();
        if matches!(state.settings.placement.foliage.mode, FoliageMode::Paint | FoliageMode::Erase) {
            changed |= Self::palette_ui(ui, state);
            ui.separator();
        }
        changed |= Self::brush_ui(ui, state);
        if let Some(r) = &self.last_report {
            ui.separator();
            ui.label(RichText::new(r.as_str()).weak());
        }
        if changed {
            state.settings_dirty = true;
        }
    }

    fn cursor(&self, _state: &AppState, _view: &ViewInfo, _pos: Pos2) -> Option<CursorIcon> {
        Some(CursorIcon::Crosshair)
    }

    fn status_hint(&self, state: &AppState) -> Option<String> {
        let f = &state.settings.placement.foliage;
        Some(match f.mode {
            FoliageMode::DetailAlpha | FoliageMode::DetailFace => t!("foliage.hint_detail").to_owned(),
            _ => match f.palette().filter(|p| p.items.iter().any(|i| i.usable())) {
                Some(p) => tf!("foliage.hint", p.name),
                None => t!("foliage.hint_empty").to_owned(),
            },
        })
    }

    fn cancel(&mut self, state: &mut AppState) -> bool {
        if self.active.take().is_some() {
            state.doc.cancel();
            return true;
        }
        self.hover = None;
        false
    }

    fn option_values(&self) -> Vec<ToolOption> {
        self.options.clone()
    }

    fn refresh_options(&mut self, state: &AppState) {
        self.options = foliage_options(&state.settings.placement.foliage);
    }

    fn set_option(&mut self, state: &mut AppState, key: &str, value: &str) -> bool {
        let f = &mut state.settings.placement.foliage;
        let num = || value.trim().parse::<f64>().ok().filter(|v| v.is_finite());
        let ok = match key {
            "mode" => FoliageMode::from_name(value).map(|m| f.mode = m).is_some(),
            "palette" => match f.palettes.iter().position(|p| p.name.eq_ignore_ascii_case(value.trim())) {
                Some(i) => {
                    f.active = i;
                    true
                }
                None => false,
            },
            "radius" => num().map(|v| f.radius = v.clamp(*RADIUS_RANGE.start(), *RADIUS_RANGE.end())).is_some(),
            "density" => num().map(|v| f.density = v.clamp(*DENSITY_RANGE.start(), *DENSITY_RANGE.end())).is_some(),
            "spacing" => num().map(|v| f.spacing = v.clamp(0.0, 4096.0)).is_some(),
            "scale_min" => num().map(|v| f.scale_min = v.clamp(0.01, 100.0)).is_some(),
            "scale_max" => num().map(|v| f.scale_max = v.clamp(0.01, 100.0)).is_some(),
            "align" => num().map(|v| f.align = v.clamp(0.0, 1.0)).is_some(),
            "slope_min" => num().map(|v| f.slope_min = v.clamp(0.0, 180.0)).is_some(),
            "slope_max" => num().map(|v| f.slope_max = v.clamp(0.0, 180.0)).is_some(),
            "height_min" => num().map(|v| f.height_min = v).is_some(),
            "height_max" => num().map(|v| f.height_max = v).is_some(),
            "sink" => num().map(|v| f.sink = v.clamp(-4096.0, 4096.0)).is_some(),
            "strength" => num().map(|v| f.detail_strength = v.clamp(0.0, 255.0)).is_some(),
            "detail_type" => {
                f.detail_type = value.trim().to_owned();
                true
            }
            "height_limit" => option_flag(value).map(|b| f.height_limit = b).is_some(),
            "random_yaw" => option_flag(value).map(|b| f.random_yaw = b).is_some(),
            "avoid_overlap" => option_flag(value).map(|b| f.avoid_overlap = b).is_some(),
            "drop_to_surface" => option_flag(value).map(|b| f.drop_to_surface = b).is_some(),
            "random_skin" => option_flag(value).map(|b| f.random_skin = b).is_some(),
            "on_props" => option_flag(value).map(|b| f.on_props = b).is_some(),
            _ => false,
        };
        if ok {
            state.settings_dirty = true;
            self.options = foliage_options(&state.settings.placement.foliage);
        }
        ok
    }
}

pub fn foliage_options(f: &FoliageSettings) -> Vec<ToolOption> {
    let n = |v: f64| format!("{}", (v * 1000.0).round() / 1000.0);
    vec![
        ToolOption::new("mode", f.mode.name(), "paint, erase, detail_alpha, detail_face"),
        ToolOption::new("palette", f.palette().map(|p| p.name.clone()).unwrap_or_default(), "a palette name"),
        ToolOption::new("radius", n(f.radius()), "4 to 8192"),
        ToolOption::new("density", n(f.density()), "props per 256 x 256 units, 0.01 to 500"),
        ToolOption::new("spacing", n(f.spacing), "0 to 4096"),
        ToolOption::new("scale_min", n(f.scale_min), "0.01 to 100"),
        ToolOption::new("scale_max", n(f.scale_max), "0.01 to 100"),
        ToolOption::new("random_yaw", on_off(f.random_yaw), "on, off"),
        ToolOption::new("align", n(f.align), "0 to 1"),
        ToolOption::new("slope_min", n(f.slope_min), "degrees 0 to 180"),
        ToolOption::new("slope_max", n(f.slope_max), "degrees 0 to 180"),
        ToolOption::new("height_limit", on_off(f.height_limit), "on, off"),
        ToolOption::new("height_min", n(f.height_min), "world Z"),
        ToolOption::new("height_max", n(f.height_max), "world Z"),
        ToolOption::new("avoid_overlap", on_off(f.avoid_overlap), "on, off"),
        ToolOption::new("drop_to_surface", on_off(f.drop_to_surface), "on, off"),
        ToolOption::new("random_skin", on_off(f.random_skin), "on, off"),
        ToolOption::new("on_props", on_off(f.on_props), "on, off"),
        ToolOption::new("sink", n(f.sink), "-4096 to 4096"),
        ToolOption::new("strength", n(f.detail_strength), "0 to 255 (detail_alpha)"),
        ToolOption::new("detail_type", f.detail_type.clone(), "a detail.vbsp group, none, or empty (detail_face)"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::{info_2d, info_3d_looking_down, settle};

    fn props(state: &AppState) -> usize {
        state.doc.scene().entities().values().filter(|n| n.entity.classname == "prop_static").count()
    }

    fn ready(state: &mut AppState) {
        let f = &mut state.settings.placement.foliage;
        f.palette_mut().add_model("models/props_foliage/bush.mdl");
        f.radius = 96.0;
        f.density = 30.0;
        f.mode = FoliageMode::Paint;
    }

    #[test]
    fn a_stroke_paints_then_shift_erases_each_one_undo_step() {
        let mut state = settle();
        ready(&mut state);
        let mut t = FoliageTool::new();
        let view = info_3d_looking_down(DVec3::new(-128.0, -128.0, 900.0));
        let p = view.world_to_screen(DVec3::new(-128.0, -128.0, 0.0)).expect("screen");
        let q = view.world_to_screen(DVec3::new(-20.0, -128.0, 0.0)).expect("screen");
        let undo = state.doc.history_len().0;
        t.handle_mouse(&mut state, &view, &Mouse::drag_start(p, p));
        assert!(t.is_painting());
        t.handle_mouse(&mut state, &view, &Mouse::drag(p, q));
        t.handle_mouse(&mut state, &view, &Mouse::drag_end(p, q));
        assert!(!t.is_painting());
        let n = props(&state);
        assert!(n > 5, "{n}");
        assert_eq!(state.doc.history_len().0, undo + 1);
        assert_eq!(state.doc.undo_label(), Some("Foliage paint"));
        assert_eq!(hp_app::foliage::layer_ids(&state.doc, Some("Foliage")).len(), 1);
        let shift = Modifiers::SHIFT;
        t.handle_mouse(&mut state, &view, &Mouse::drag_start(p, p).with_modifiers(shift));
        t.handle_mouse(&mut state, &view, &Mouse::drag_end(p, p).with_modifiers(shift));
        let left = props(&state);
        assert!(left < n, "{left} of {n}");
        assert_eq!(state.doc.undo_label(), Some("Foliage erase"));
        assert!(state.doc.undo());
        assert_eq!(props(&state), n);
    }

    #[test]
    fn the_top_view_paints_and_an_empty_palette_refuses() {
        let mut state = settle();
        let mut t = FoliageTool::new();
        let view = info_2d(ViewKind::Top, 1.0);
        let p = view.world_to_screen(DVec3::new(-150.0, -150.0, 0.0)).expect("p");
        state.settings.placement.foliage.palettes[0].items.clear();
        t.handle_mouse(&mut state, &view, &Mouse::click(p));
        assert_eq!(props(&state), 0);
        ready(&mut state);
        t.handle_mouse(&mut state, &view, &Mouse::click(p));
        assert!(props(&state) > 0);
        let front = info_2d(ViewKind::Front, 1.0);
        let before = props(&state);
        t.handle_mouse(&mut state, &front, &Mouse::click(p));
        assert_eq!(props(&state), before, "front and side views do not paint");
    }

    #[test]
    fn detail_modes_paint_alpha_and_face_overrides() {
        let mut state = settle();
        state.doc.create_displacements(&[FaceRef::new(2, 0)], 2).expect("disp");
        let mut t = FoliageTool::new();
        state.settings.placement.foliage.mode = FoliageMode::DetailAlpha;
        state.settings.placement.foliage.radius = 300.0;
        state.settings.placement.foliage.detail_strength = 100.0;
        let view = info_3d_looking_down(DVec3::new(0.0, 0.0, 900.0));
        let p = view.world_to_screen(DVec3::new(0.0, 0.0, 0.0)).expect("screen");
        t.handle_mouse(&mut state, &view, &Mouse::click(p));
        let alphas =
            |s: &AppState| s.doc.scene().solid(2).and_then(|n| n.solid.sides[0].disp.clone()).expect("d").alphas;
        assert!(alphas(&state).iter().any(|a| *a > 50.0));
        assert_eq!(state.doc.undo_label(), Some("Detail paint"));
        t.handle_mouse(&mut state, &view, &Mouse::click(p).with_modifiers(Modifiers::SHIFT));
        assert!(alphas(&state).iter().all(|a| *a < 1.0));

        state.settings.placement.foliage.mode = FoliageMode::DetailFace;
        state.settings.placement.foliage.detail_type = "grassland".to_owned();
        t.handle_mouse(&mut state, &view, &Mouse::click(p));
        let side =
            |s: &AppState| s.doc.scene().solid(2).map(|n| n.solid.sides[0].extra_props.clone()).unwrap_or_default();
        assert!(side(&state).iter().any(|(k, v)| k == OVERRIDE_KEY && v == "grassland"), "{:?}", side(&state));
        t.handle_mouse(&mut state, &view, &Mouse::click(p).with_modifiers(Modifiers::SHIFT));
        assert!(!side(&state).iter().any(|(k, _)| k == OVERRIDE_KEY));
    }

    #[test]
    fn options_round_trip() {
        let mut state = settle();
        let mut t = FoliageTool::new();
        assert!(t.set_option(&mut state, "mode", "detail_face"));
        assert!(t.set_option(&mut state, "density", "12.5"));
        assert!(t.set_option(&mut state, "avoid_overlap", "off"));
        assert!(!t.set_option(&mut state, "nope", "1"));
        assert!(!t.set_option(&mut state, "palette", "missing"));
        let opts = t.option_values();
        let get = |k: &str| opts.iter().find(|o| o.key == k).map(|o| o.value.clone()).unwrap_or_default();
        assert_eq!(get("mode"), "detail_face");
        assert_eq!(get("density"), "12.5");
        assert_eq!(get("avoid_overlap"), "off");
    }
}
