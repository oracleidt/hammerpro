use std::cell::RefCell;
use std::collections::HashMap;

use egui::{Align2, Color32, FontId, Pos2, vec2};
use glam::DVec3;
use hp_app::AppState;
use hp_app::i18n::TrTable;
use hp_app::runtime::helpers::geometry::entity_color;
use hp_app::tf;
use hp_app::view::ViewInfo;
use hp_editor::display_color::chosen;
use hp_editor::{Id, ObjRef, PickOptions};
use hp_geom::Ray;

use super::OverlayLayer;
use crate::overlay::pixel_world_size;
use crate::paint::{PILL_BG, TEXT_WEAK, small_font, text_pill};

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(LabelsLayer::default())]
}

pub(crate) const TR: TrTable =
    &[("viewport.names_hidden", "{0} entity names: zoom in to see them", "{0} noms d'entités : zoomez pour les voir")];

pub const MAX_NAMES: usize = 1500;
const CLASS_ZOOM: f64 = 1.0;
const MAX_LABEL: usize = 64;
const MAX_TEXT_PX: f32 = 40.0;
const MAX_OCCLUSION_TESTS: usize = 64;

#[derive(Clone, Copy, Debug)]
struct NameEntry {
    id: Id,
    pos: DVec3,
    color: [u8; 3],
}

#[derive(Clone, Debug)]
struct TextEntry {
    pos: DVec3,
    text: String,
    size: f64,
    color: [u8; 3],
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct NamesKey {
    stamp: (u64, u64, bool),
    kind: u8,
    center: [u64; 2],
    zoom: u64,
    rect: [u32; 4],
    pixels_per_point: u32,
}

type NamesCache = [Option<(NamesKey, Vec<egui::Shape>)>; 4];

#[derive(Default)]
struct LabelsLayer {
    names_cache: RefCell<NamesCache>,
    stamp: Option<(u64, u64, bool)>,
    names: Vec<NameEntry>,
    texts: Vec<TextEntry>,
    show_names: bool,
    show_texts: bool,
    worldtext: HashMap<String, bool>,
    scratch: RefCell<Vec<(usize, Pos2)>>,
}

fn color32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn soften(c: [u8; 3]) -> Color32 {
    let mix = |a: u8, b: u8| ((u16::from(a) * 3 + u16::from(b)) / 4) as u8;
    let w = TEXT_WEAK;
    Color32::from_rgb(mix(c[0], w.r()), mix(c[1], w.g()), mix(c[2], w.b()))
}

fn short(s: &str) -> String {
    if s.chars().count() <= MAX_LABEL {
        s.to_owned()
    } else {
        let mut out: String = s.chars().take(MAX_LABEL - 3).collect();
        out.push_str("...");
        out
    }
}

impl LabelsLayer {
    fn rebuild(&mut self, state: &AppState) {
        let doc = &state.doc;
        let fgd = state.assets.fgd();
        let scene = doc.scene();
        self.names.clear();
        self.texts.clear();
        for (id, node) in scene.entities() {
            if scene.is_brush_entity(*id) || !doc.is_entity_visible(*id) {
                continue;
            }
            let e = &node.entity;
            let Some(pos) = e.origin().filter(|p| p.is_finite()) else { continue };
            let class = fgd.and_then(|f| f.class(&e.classname));
            let chosen = state.settings.render.object_colors_2d.then(|| chosen(e.editor.color)).flatten();
            let color = chosen.unwrap_or_else(|| entity_color(e, class));
            self.names.push(NameEntry { id: *id, pos, color });
            let worldtext = match self.worldtext.get(e.classname.as_str()) {
                Some(w) => *w,
                None => {
                    let w = class.is_some_and(|c| c.has_helper("worldtext"))
                        || ["editor_text", "point_worldtext"]
                            .iter()
                            .any(|c| e.classname.trim().eq_ignore_ascii_case(c));
                    self.worldtext.insert(e.classname.clone(), w);
                    w
                }
            };
            if worldtext && let Some(msg) = e.get("message").map(str::trim).filter(|m| !m.is_empty()) {
                let size = e
                    .get("textsize")
                    .and_then(|s| s.trim().parse::<f64>().ok())
                    .filter(|s| s.is_finite() && *s > 0.0)
                    .unwrap_or(10.0)
                    .min(1024.0);
                let rgb = |k: &str| {
                    let v: Vec<u8> = e
                        .get(k)?
                        .split_ascii_whitespace()
                        .take(3)
                        .filter_map(|t| t.parse::<f64>().ok())
                        .filter(|c| c.is_finite())
                        .map(|c| c.round().clamp(0.0, 255.0) as u8)
                        .collect();
                    (v.len() == 3).then(|| [v[0], v[1], v[2]])
                };
                let color = rgb("color").or_else(|| rgb("rendercolor")).unwrap_or([255; 3]);
                self.texts.push(TextEntry { pos, text: msg.chars().take(256).collect(), size, color });
            }
        }
    }

    fn names_key(&self, view: &ViewInfo) -> Option<NamesKey> {
        let r = view.rect;
        Some(NamesKey {
            stamp: self.stamp?,
            kind: view.kind as u8,
            center: [view.cam2d.center.x.to_bits(), view.cam2d.center.y.to_bits()],
            zoom: view.cam2d.zoom.to_bits(),
            rect: [r.min.x.to_bits(), r.min.y.to_bits(), r.max.x.to_bits(), r.max.y.to_bits()],
            pixels_per_point: view.pixels_per_point.to_bits(),
        })
    }

    fn paint_names(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let key = self.names_key(view);
        let mut cache = self.names_cache.borrow_mut();
        let slot = cache.get_mut(view.index);
        if let (Some(key), Some(Some((cached, shapes)))) = (key, slot.as_deref())
            && *cached == key
        {
            painter.extend(shapes.iter().cloned());
            return;
        }
        let Some(shapes) = self.layout_names(state, view, painter) else {
            if let Some(slot) = cache.get_mut(view.index) {
                *slot = None;
            }
            return;
        };
        painter.extend(shapes.iter().cloned());
        if let (Some(key), Some(slot)) = (key, cache.get_mut(view.index)) {
            *slot = Some((key, shapes));
        }
    }

    fn layout_names(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) -> Option<Vec<egui::Shape>> {
        let mut shapes = Vec::new();
        let rect = view.rect.expand(40.0);
        let mut visible = self.scratch.borrow_mut();
        visible.clear();
        for (i, n) in self.names.iter().enumerate() {
            if let Some(p) = view.world_to_screen(n.pos)
                && rect.contains(p)
            {
                visible.push((i, p));
            }
        }
        let doc = &state.doc;
        let zoomed = view.cam2d.zoom >= CLASS_ZOOM;
        if !zoomed {
            visible.retain(|(i, _)| {
                doc.scene()
                    .entity(self.names[*i].id)
                    .is_some_and(|n| n.entity.get("targetname").is_some_and(|t| !t.trim().is_empty()))
            });
        }
        if visible.len() > MAX_NAMES {
            let pos = view.rect.left_bottom() + vec2(8.0, -8.0);
            text_pill(painter, pos, Align2::LEFT_BOTTOM, tf!("viewport.names_hidden", visible.len()), TEXT_WEAK);
            return None;
        }
        let font = small_font();
        let offset = (8.0 * view.cam2d.zoom).clamp(2.0, 64.0) as f32 + 3.0;
        for (i, p) in visible.iter() {
            let n = &self.names[*i];
            let Some(node) = doc.scene().entity(n.id) else { continue };
            let e = &node.entity;
            let name = e.get("targetname").map(str::trim).filter(|t| !t.is_empty());
            let text = match (name, zoomed) {
                (Some(name), true) => format!("{} ({})", short(name), short(e.classname.trim())),
                (Some(name), false) => short(name),
                (None, true) => short(e.classname.trim()),
                (None, false) => continue,
            };
            let selected = doc.selection().contains(ObjRef::Entity(n.id));
            let color = if selected { Color32::WHITE } else { soften(n.color) };
            let at = *p + vec2(offset, 0.0);
            let galley = painter.layout_no_wrap(text, font.clone(), color);
            let r = Align2::LEFT_CENTER.anchor_size(at, galley.size());
            if selected {
                shapes.push(egui::Shape::rect_filled(r.expand(2.0), 3, PILL_BG));
            }
            shapes.push(egui::Shape::galley(r.min, galley, color));
        }
        Some(shapes)
    }

    fn paint_texts(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        let mut tested = 0;
        for t in &self.texts {
            let Some(p) = view.world_to_screen(t.pos) else { continue };
            if !view.rect.expand(200.0).contains(p) {
                continue;
            }
            let px = if view.is_2d() {
                12.0
            } else {
                let world_px = pixel_world_size(view, t.pos);
                if !(world_px.is_finite() && world_px > 0.0) {
                    continue;
                }
                if tested < MAX_OCCLUSION_TESTS {
                    tested += 1;
                    if occluded(state, view.cam3d.pos, t.pos) {
                        continue;
                    }
                }
                (t.size / world_px / f64::from(view.pixels_per_point.max(0.1))) as f32
            };
            if px < 5.0 {
                continue;
            }
            let font = FontId::proportional(px.min(MAX_TEXT_PX));
            painter.text(p, Align2::CENTER_CENTER, &t.text, font, color32(t.color));
        }
    }
}

fn occluded(state: &AppState, eye: DVec3, p: DVec3) -> bool {
    let d = p - eye;
    let dist = d.length();
    let Some(dir) = d.try_normalize() else { return false };
    if !(dist.is_finite() && eye.is_finite()) {
        return false;
    }
    let opts = PickOptions { include_entities: false, max_distance: dist, ..PickOptions::default() };
    state.doc.pick_ray(&Ray::new(eye, dir), &opts, &|e| state.entity_bounds(e)).is_some_and(|hit| hit.t < dist - 1.0)
}

impl OverlayLayer for LabelsLayer {
    fn name(&self) -> &'static str {
        "labels"
    }

    fn prepare(&mut self, state: &AppState) {
        let h = &state.settings.helpers;
        self.show_names = h.show_entity_names;
        self.show_texts = state.settings.view.show_helpers && h.show_editor_objects;
        if !self.show_names && !self.show_texts {
            return;
        }
        let generation = state.assets.generation();
        let stamp = (state.doc.revision(), generation, state.settings.render.object_colors_2d);
        if self.stamp.is_some_and(|s| s.1 != generation) {
            self.worldtext.clear();
        }
        if self.stamp != Some(stamp) {
            self.rebuild(state);
            self.stamp = Some(stamp);
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if self.show_names && view.is_2d() {
            self.paint_names(state, view, painter);
        }
        if self.show_texts {
            self.paint_texts(state, view, painter);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, pos2};
    use glam::DVec2;
    use hp_app::view::{Camera2d, Camera3d, ViewKind};

    fn view(kind: ViewKind, zoom: f64) -> ViewInfo {
        ViewInfo {
            index: 0,
            kind,
            rect: Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0)),
            cam3d: Camera3d { pos: DVec3::new(-400.0, 0.0, 96.0), yaw_deg: 0.0, pitch_deg: 0.0, ..Camera3d::default() },
            cam2d: Camera2d { center: DVec2::ZERO, zoom },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    fn shapes(state: &AppState, layer: &LabelsLayer, v: &ViewInfo) -> usize {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            layer.paint(state, v, ui.painter());
        });
        out.textures_delta.clear();
        out.shapes.len()
    }

    #[test]
    fn names_are_laid_out_once_while_the_view_stays_still() {
        let mut state = hp_app::testing::test_state();
        let mut layer = LabelsLayer::default();
        layer.prepare(&state);
        let v = view(ViewKind::Top, 2.0);
        let first = shapes(&state, &layer, &v);
        assert!(first > 0);
        assert!(layer.names_cache.borrow()[0].is_some(), "the laid out names are kept");
        let key = layer.names_cache.borrow()[0].as_ref().map(|(k, _)| *k);
        assert_eq!(shapes(&state, &layer, &v), first);
        assert_eq!(layer.names_cache.borrow()[0].as_ref().map(|(k, _)| *k), key);
        let mut moved = v;
        moved.cam2d.center.x += 16.0;
        assert_eq!(shapes(&state, &layer, &moved), first);
        assert_ne!(layer.names_cache.borrow()[0].as_ref().map(|(k, _)| *k), key, "panning lays them out again");
        let ids: Vec<Id> = state.doc.scene().entities().keys().copied().collect();
        state.doc.set_entity_key(&ids, "targetname", "renamed").expect("rename");
        layer.prepare(&state);
        assert_eq!(shapes(&state, &layer, &moved), first, "an edit refreshes the names");
    }

    fn layer_note(state: &AppState) -> Id {
        let found = state.doc.scene().entities().iter().find(|(_, n)| n.entity.classname == "editor_text");
        found.map(|(id, _)| *id).expect("note")
    }

    #[test]
    fn names_and_notes() {
        let mut state = hp_app::testing::test_state();
        state
            .doc
            .create_point_entity(
                "editor_text",
                DVec3::new(32.0, 0.0, 64.0),
                &[("message".to_owned(), "Fix the door".to_owned()), ("textsize".to_owned(), "16".to_owned())],
            )
            .expect("note");
        let mut layer = LabelsLayer::default();
        layer.prepare(&state);
        assert_eq!(layer.names.len(), 3, "player start, light and note");
        assert_eq!(layer.texts.len(), 1);
        assert_eq!(layer.texts[0].text, "Fix the door");
        let out = shapes(&state, &layer, &view(ViewKind::Top, 0.5));
        let zoomed = shapes(&state, &layer, &view(ViewKind::Top, 2.0));
        assert!(zoomed > out, "{zoomed} {out}");
        assert_eq!(shapes(&state, &layer, &view(ViewKind::Perspective, 1.0)), 1, "only the note in 3D");
        let entity_count = state.doc.scene().entities().len();
        state.settings.helpers.show_entity_names = false;
        layer.prepare(&state);
        assert_eq!(shapes(&state, &layer, &view(ViewKind::Top, 0.5)), 1, "the map note remains visible");
        assert_eq!(shapes(&state, &layer, &view(ViewKind::Perspective, 1.0)), 1, "3D text is unchanged");
        assert_eq!(state.doc.scene().entities().len(), entity_count);
        state.settings.helpers.show_entity_names = true;
        let mut behind = view(ViewKind::Perspective, 1.0);
        behind.cam3d =
            Camera3d { pos: DVec3::new(32.0, -200.0, 64.0), yaw_deg: 90.0, pitch_deg: 0.0, ..Camera3d::default() };
        state.doc.edit_entity(layer_note(&state), |e| e.set("origin", "32 300 64")).expect("move");
        layer.prepare(&state);
        assert_eq!(shapes(&state, &layer, &behind), 0, "occluded by the wall");
        state.settings.helpers.show_entity_names = false;
        state.settings.view.show_helpers = false;
        layer.prepare(&state);
        assert!(!layer.show_names && !layer.show_texts);
        assert_eq!(short(&"x".repeat(100)).chars().count(), MAX_LABEL);
    }
}
