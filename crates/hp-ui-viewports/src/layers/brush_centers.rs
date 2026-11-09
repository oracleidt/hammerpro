use std::cell::RefCell;

use egui::{Color32, Stroke, vec2};
use glam::DVec3;
use hp_app::{AppState, view::ViewInfo};
use hp_editor::{Id, ObjRef, Owner};

use super::OverlayLayer;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(BrushCenters::default())]
}

#[derive(Default)]
struct Centers {
    revision: Option<u64>,
    entries: Vec<(Id, DVec3, Owner)>,
}

#[derive(Default)]
struct BrushCenters {
    centers: RefCell<Centers>,
}

impl Centers {
    fn sync(&mut self, state: &AppState) {
        let revision = state.doc.revision();
        if self.revision == Some(revision) {
            return;
        }
        self.entries.clear();
        self.entries.extend(state.doc.scene().solids().iter().filter_map(|(id, node)| {
            (!node.bounds.is_empty())
                .then(|| (*id, node.bounds.center(), node.owner))
                .filter(|(_, center, _)| center.is_finite())
        }));
        if self.entries.capacity() > 4096 && self.entries.len() < self.entries.capacity() / 4 {
            self.entries.shrink_to_fit();
        }
        self.revision = Some(revision);
    }
}

impl OverlayLayer for BrushCenters {
    fn name(&self) -> &'static str {
        "brush_centers"
    }

    fn prepare(&mut self, state: &AppState) {
        if !state.settings.helpers.show_brush_centers {
            *self.centers.get_mut() = Centers::default();
        }
    }

    fn paint(&self, state: &AppState, view: &ViewInfo, painter: &egui::Painter) {
        if !view.is_2d() || !state.settings.helpers.show_brush_centers || state.runtime.lab.player.active {
            return;
        }
        let mut centers = self.centers.borrow_mut();
        centers.sync(state);
        let painter = painter.with_clip_rect(view.rect.intersect(painter.clip_rect()));
        let selected = state.settings.view.selection_color;
        for &(id, center, owner) in &centers.entries {
            let Some(point) = view.world_to_screen(center) else { continue };
            if !view.rect.expand(4.0).contains(point) || !state.doc.is_solid_pickable(id) {
                continue;
            }
            let chosen = state.doc.is_selected(ObjRef::Solid(id))
                || matches!(owner, Owner::Entity(entity) if state.doc.is_selected(ObjRef::Entity(entity)));
            let color =
                if chosen { Color32::from_rgb(selected[0], selected[1], selected[2]) } else { Color32::from_gray(185) };
            let stroke = Stroke::new(1.0, color);
            let a = vec2(3.0, 3.0);
            let b = vec2(3.0, -3.0);
            painter.line_segment([point - a, point + a], stroke);
            painter.line_segment([point - b, point + b], stroke);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Rect, Shape, pos2, vec2};
    use hp_app::view::{Camera2d, Camera3d, ViewKind};
    use hp_editor::Document;

    fn state() -> (AppState, Id, Id) {
        let mut state = hp_app::testing::test_state();
        let solid = hp_vmf::parse(hp_app::testing::TEST_VMF).unwrap().world.solids.remove(0).sides;
        state.doc = Document::new_empty();
        let world = state.doc.create_solid(solid.clone(), Owner::World).unwrap();
        let entity = state.doc.create_brush_entity("func_brush", &[], vec![solid]).unwrap();
        state.doc.translate(&[ObjRef::Entity(entity)], DVec3::new(128.0, 128.0, 64.0)).unwrap();
        state.doc.clear_selection();
        (state, world, entity)
    }

    fn view(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 0,
            kind,
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(1600.0, 1200.0)),
            cam3d: Camera3d::default(),
            cam2d: Camera2d::default(),
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    fn drawn(layer: &BrushCenters, state: &AppState, view: &ViewInfo) -> Vec<Shape> {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput { screen_rect: Some(view.rect), ..Default::default() }, |ui| {
            layer.paint(state, view, ui.painter());
        });
        output.shapes.into_iter().map(|s| s.shape).collect()
    }

    #[test]
    fn brush_centers_are_default_crosses_for_world_and_entity_brushes_in_every_2d_axis() {
        let (state, _, _) = state();
        let layer = BrushCenters::default();
        assert!(state.settings.helpers.show_brush_centers);
        assert!(drawn(&layer, &state, &view(ViewKind::Perspective)).is_empty());
        assert!(layer.centers.borrow().entries.is_empty());
        for kind in [ViewKind::Top, ViewKind::Front, ViewKind::Side] {
            let view = view(kind);
            let shapes = drawn(&layer, &state, &view);
            assert_eq!(shapes.len(), 4);
            let centers: Vec<_> = state
                .doc
                .scene()
                .solids()
                .values()
                .map(|node| view.world_to_screen(node.bounds.center()).unwrap())
                .collect();
            for shape in shapes {
                let Shape::LineSegment { points, .. } = shape else { panic!("a center is a line segment") };
                let midpoint = points[0] + (points[1] - points[0]) * 0.5;
                assert!(centers.iter().any(|center| center.distance(midpoint) < 0.001));
                assert!((points[0].distance(points[1]) - 72f32.sqrt()).abs() < 0.001);
            }
        }
    }

    #[test]
    fn brush_centers_follow_visibility_edits_undo_zoom_and_the_saved_toggle() {
        let (mut state, world, entity) = state();
        let mut layer = BrushCenters::default();
        let mut view = view(ViewKind::Top);
        assert_eq!(drawn(&layer, &state, &view).len(), 4);
        state.doc.set_hidden(&[ObjRef::Solid(world)], true);
        assert_eq!(drawn(&layer, &state, &view).len(), 2);
        assert!(state.doc.undo());
        assert_eq!(drawn(&layer, &state, &view).len(), 4);
        state.doc.set_hidden(&[ObjRef::Entity(entity)], true);
        assert_eq!(drawn(&layer, &state, &view).len(), 2);
        assert!(state.doc.undo());
        state.doc.translate(&[ObjRef::Solid(world)], DVec3::new(64.0, 0.0, 0.0)).unwrap();
        let shapes = drawn(&layer, &state, &view);
        let actual = state.doc.scene().solid(world).unwrap().bounds.center();
        assert!(layer.centers.borrow().entries.iter().any(|&(id, p, _)| id == world && p == actual));
        assert_eq!(shapes.len(), 4);
        view.cam2d.zoom = 0.25;
        for shape in drawn(&layer, &state, &view) {
            let Shape::LineSegment { points, .. } = shape else { panic!("a center is a line segment") };
            assert!((points[0].distance(points[1]) - 72f32.sqrt()).abs() < 0.001);
        }
        state.settings.helpers.show_brush_centers = false;
        layer.prepare(&state);
        assert!(drawn(&layer, &state, &view).is_empty());
        assert_eq!(layer.centers.borrow().entries.capacity(), 0);
    }
}
