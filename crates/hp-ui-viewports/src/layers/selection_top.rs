use std::cell::RefCell;
use std::collections::HashSet;

use egui::{Color32, Pos2, Shape, Stroke};
use glam::DVec3;
use hp_app::AppState;
use hp_app::view::ViewInfo;
use hp_editor::ObjRef;

use super::OverlayLayer;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(SelectionTopLayer::default())]
}

pub const MAX_SEGMENTS: usize = 40_000;

#[derive(Clone, Copy, Debug, PartialEq)]
struct OutlineKey {
    revision: u64,
    kind: u8,
    center: [u64; 2],
    zoom: u64,
    rect: [u32; 4],
    pixels_per_point: u32,
    color: [u8; 3],
}

type OutlineCache = [Option<(OutlineKey, Vec<Shape>)>; 4];

#[derive(Default)]
struct SelectionTopLayer {
    cache: RefCell<OutlineCache>,
}

fn key(state: &AppState, view: &ViewInfo) -> OutlineKey {
    let r = view.rect;
    OutlineKey {
        revision: state.doc.revision(),
        kind: view.kind as u8,
        center: [view.cam2d.center.x.to_bits(), view.cam2d.center.y.to_bits()],
        zoom: view.cam2d.zoom.to_bits(),
        rect: [r.min.x.to_bits(), r.min.y.to_bits(), r.max.x.to_bits(), r.max.y.to_bits()],
        pixels_per_point: view.pixels_per_point.to_bits(),
        color: state.settings.view.selection_color,
    }
}

fn quantize(p: Pos2) -> (i32, i32) {
    ((p.x * 4.0).round() as i32, (p.y * 4.0).round() as i32)
}

struct Segments {
    seen: HashSet<((i32, i32), (i32, i32))>,
    list: Vec<[Pos2; 2]>,
    clip: egui::Rect,
}

impl Segments {
    fn push(&mut self, a: Pos2, b: Pos2) {
        if self.list.len() >= MAX_SEGMENTS
            || !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite())
        {
            return;
        }
        let seg = egui::Rect::from_two_pos(a, b);
        if !seg.expand(2.0).intersects(self.clip) {
            return;
        }
        let (qa, qb) = (quantize(a), quantize(b));
        if qa == qb {
            return;
        }
        let k = if qa <= qb { (qa, qb) } else { (qb, qa) };
        if self.seen.insert(k) {
            self.list.push([a, b]);
        }
    }

    fn polygon(&mut self, view: &ViewInfo, pts: &[DVec3]) {
        let screen: Vec<Pos2> = pts.iter().filter_map(|p| view.world_to_screen(*p)).collect();
        if screen.len() != pts.len() {
            return;
        }
        for (i, a) in screen.iter().enumerate() {
            self.push(*a, screen[(i + 1) % screen.len()]);
        }
    }
}

pub fn outline_segments(state: &AppState, view: &ViewInfo) -> Vec<[Pos2; 2]> {
    let mut segs = Segments { seen: HashSet::new(), list: Vec::new(), clip: view.rect };
    let doc = &state.doc;
    let scene = doc.scene();
    let objects = doc.selected_objects();
    for id in doc.selected_solid_ids() {
        let Some(node) = scene.solid(id) else { continue };
        for face in &node.geom.faces {
            segs.polygon(view, &face.vertices);
        }
    }
    for obj in objects {
        let ObjRef::Entity(id) = obj else { continue };
        if scene.is_brush_entity(id) {
            continue;
        }
        let Some(node) = scene.entity(id) else { continue };
        let Some(b) = state.entity_bounds(&node.entity) else { continue };
        let c = b.corners();
        for (i, j) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
            segs.polygon(view, &[c[i], c[j]]);
        }
    }
    segs.list
}

fn build(state: &AppState, view: &ViewInfo) -> Vec<Shape> {
    let segs = outline_segments(state, view);
    if segs.is_empty() {
        return Vec::new();
    }
    let [r, g, b] = state.settings.view.selection_color;
    let color = Color32::from_rgb(r, g, b);
    let halo = Stroke::new(3.0, Color32::from_black_alpha(150));
    let line = Stroke::new(1.25, color);
    let mut out = Vec::with_capacity(segs.len() * 2);
    out.extend(segs.iter().map(|s| Shape::line_segment(*s, halo)));
    out.extend(segs.iter().map(|s| Shape::line_segment(*s, line)));
    out
}

impl OverlayLayer for SelectionTopLayer {
    fn name(&self) -> &'static str {
        "selection_top"
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if !view.is_2d() || state.doc.selection().objects.is_empty() {
            return;
        }
        let k = key(state, view);
        let slot = view.index.min(3);
        let mut cache = self.cache.borrow_mut();
        let fresh = match &cache[slot] {
            Some((old, _)) => *old != k,
            None => true,
        };
        if fresh {
            cache[slot] = Some((k, build(state, view)));
        }
        if let Some((_, shapes)) = &cache[slot] {
            painter.extend(shapes.iter().cloned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, pos2};
    use glam::DVec2;
    use hp_app::view::{Camera2d, Camera3d, ViewKind};

    fn view(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 1,
            kind,
            rect: Rect::from_min_size(pos2(0.0, 0.0), egui::vec2(800.0, 600.0)),
            cam3d: Camera3d::default(),
            cam2d: Camera2d { center: DVec2::ZERO, zoom: 0.5 },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    fn painted(state: &AppState, layer: &SelectionTopLayer, v: &ViewInfo) -> usize {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            layer.paint(state, v, ui.painter());
        });
        out.textures_delta.clear();
        out.shapes.len()
    }

    #[test]
    fn selection_outline_is_painted_on_top_in_2d_only() {
        let mut state = hp_app::testing::test_state();
        let layer = SelectionTopLayer::default();
        let top = view(ViewKind::Top);
        assert_eq!(painted(&state, &layer, &top), 0);
        let all: Vec<ObjRef> = state.doc.scene().solids().keys().map(|id| ObjRef::Solid(*id)).collect();
        state.doc.set_selection(&all);
        let segs = outline_segments(&state, &top);
        assert!(!segs.is_empty());
        let unique: HashSet<_> = segs.iter().map(|s| (quantize(s[0]), quantize(s[1]))).collect();
        assert_eq!(unique.len(), segs.len(), "shared edges are drawn once");
        assert_eq!(painted(&state, &layer, &top), segs.len() * 2);
        assert!(layer.cache.borrow()[1].is_some(), "kept while the view stays still");
        assert_eq!(painted(&state, &layer, &view(ViewKind::Perspective)), 0);
        state.doc.set_selection(&[]);
        assert_eq!(painted(&state, &layer, &top), 0);
    }
}
