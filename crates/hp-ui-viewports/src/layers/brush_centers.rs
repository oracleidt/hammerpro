use std::cell::RefCell;

use glam::DVec3;
use hp_app::{AppState, view::ViewInfo};
use hp_editor::{Id, ObjRef, Owner};
use hp_render::{Overlay, OverlayLine};

use super::OverlayLayer;
use crate::overlay::{pixel_world_size, screen_axes};

const ARM_PX: f64 = 3.0;
const IDLE_GRAY: f32 = 185.0 / 255.0;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(BrushCenters::default())]
}

#[derive(Default)]
struct Centers {
    revision: Option<u64>,
    entries: Vec<(Id, DVec3, bool)>,
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
        let doc = &state.doc;
        self.entries.clear();
        self.entries.extend(doc.scene().solids().iter().filter_map(|(id, node)| {
            let center = node.bounds.center();
            if node.bounds.is_empty() || !center.is_finite() || !doc.is_solid_pickable(*id) {
                return None;
            }
            let chosen = doc.is_selected(ObjRef::Solid(*id))
                || matches!(node.owner, Owner::Entity(entity) if doc.is_selected(ObjRef::Entity(entity)));
            Some((*id, center, chosen))
        }));
        if self.entries.capacity() > 4096 && self.entries.len() < self.entries.capacity() / 4 {
            self.entries.shrink_to_fit();
        }
        self.revision = Some(revision);
    }
}

fn shown(state: &AppState, view: &ViewInfo) -> bool {
    view.is_2d() && state.settings.helpers.show_brush_centers && !state.runtime.lab.player.active
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

    fn retained_version(&self, state: &AppState, view: &ViewInfo) -> Option<u64> {
        if !shown(state, view) {
            return None;
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(
            &(
                state.doc.revision(),
                view.kind as u8,
                view.cam2d.zoom.to_bits(),
                view.pixels_per_point.to_bits(),
                state.settings.view.selection_color,
            ),
            &mut h,
        );
        Some(std::hash::Hasher::finish(&h))
    }

    fn retained(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if !shown(state, view) {
            return;
        }
        let mut centers = self.centers.borrow_mut();
        centers.sync(state);
        let (right, up) = screen_axes(view);
        let px = pixel_world_size(view, DVec3::ZERO) * f64::from(view.pixels_per_point.max(0.1)) * ARM_PX;
        let a = (right + up) * px;
        let b = (right - up) * px;
        let s = state.settings.view.selection_color;
        let selected = [f32::from(s[0]) / 255.0, f32::from(s[1]) / 255.0, f32::from(s[2]) / 255.0, 1.0];
        let idle = [IDLE_GRAY, IDLE_GRAY, IDLE_GRAY, 1.0];
        let f = |v: DVec3| v.as_vec3().to_array();
        out.lines_on_top.reserve(centers.entries.len() * 2);
        for &(_, center, chosen) in &centers.entries {
            let color = if chosen { selected } else { idle };
            out.lines_on_top.push(OverlayLine { a: f(center - a), b: f(center + a), color });
            out.lines_on_top.push(OverlayLine { a: f(center - b), b: f(center + b), color });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Pos2, Rect, pos2, vec2};
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

    fn drawn(layer: &BrushCenters, state: &AppState, view: &ViewInfo) -> Vec<OverlayLine> {
        let mut out = Overlay::default();
        if layer.retained_version(state, view).is_some() {
            layer.retained(state, view, &mut out);
        }
        assert!(out.lines.is_empty() && out.triangles.is_empty() && out.points.is_empty());
        out.lines_on_top
    }

    fn on_screen(view: &ViewInfo, line: &OverlayLine) -> [Pos2; 2] {
        [line.a, line.b].map(|p| view.world_to_screen(glam::Vec3::from(p).as_dvec3()).unwrap())
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
            let lines = drawn(&layer, &state, &view);
            assert_eq!(lines.len(), 4);
            let centers: Vec<_> = state
                .doc
                .scene()
                .solids()
                .values()
                .map(|node| view.world_to_screen(node.bounds.center()).unwrap())
                .collect();
            for line in &lines {
                let points = on_screen(&view, line);
                let midpoint = points[0] + (points[1] - points[0]) * 0.5;
                assert!(centers.iter().any(|center| center.distance(midpoint) < 0.01));
                assert!((points[0].distance(points[1]) - 72f32.sqrt()).abs() < 0.01);
                assert!((line.color[0] - 185.0 / 255.0).abs() < 1e-6, "unselected crosses are light gray");
            }
        }
    }

    #[test]
    fn brush_centers_follow_visibility_edits_undo_zoom_selection_and_the_saved_toggle() {
        let (mut state, world, entity) = state();
        let mut layer = BrushCenters::default();
        let mut view = view(ViewKind::Top);
        let first = layer.retained_version(&state, &view);
        assert_eq!(drawn(&layer, &state, &view).len(), 4);
        state.doc.set_hidden(&[ObjRef::Solid(world)], true);
        assert_ne!(layer.retained_version(&state, &view), first, "an edit refills the overlay");
        assert_eq!(drawn(&layer, &state, &view).len(), 2);
        assert!(state.doc.undo());
        assert_eq!(drawn(&layer, &state, &view).len(), 4);
        state.doc.set_hidden(&[ObjRef::Entity(entity)], true);
        assert_eq!(drawn(&layer, &state, &view).len(), 2);
        assert!(state.doc.undo());
        state.doc.translate(&[ObjRef::Solid(world)], DVec3::new(64.0, 0.0, 0.0)).unwrap();
        let lines = drawn(&layer, &state, &view);
        let actual = state.doc.scene().solid(world).unwrap().bounds.center();
        assert!(layer.centers.borrow().entries.iter().any(|&(id, p, _)| id == world && p == actual));
        assert_eq!(lines.len(), 4);
        let before_pan = layer.retained_version(&state, &view);
        view.cam2d.center += glam::DVec2::new(300.0, -40.0);
        assert_eq!(layer.retained_version(&state, &view), before_pan, "panning reuses the uploaded crosses");
        view.cam2d.zoom *= 0.5;
        assert_ne!(layer.retained_version(&state, &view), before_pan, "zooming prepares them again");
        for line in drawn(&layer, &state, &view) {
            let points = on_screen(&view, &line);
            assert!((points[0].distance(points[1]) - 72f32.sqrt()).abs() < 0.01, "the same size at every zoom");
        }
        state.doc.select(&[ObjRef::Entity(entity)], hp_editor::SelectOp::Replace);
        let s = state.settings.view.selection_color;
        let lit = drawn(&layer, &state, &view)
            .iter()
            .filter(|l| {
                (l.color[0] - f32::from(s[0]) / 255.0).abs() < 1e-6
                    && (l.color[1] - f32::from(s[1]) / 255.0).abs() < 1e-6
            })
            .count();
        assert_eq!(lit, 2, "the selected brush entity's cross takes the selection color");
        state.settings.helpers.show_brush_centers = false;
        layer.prepare(&state);
        assert!(layer.retained_version(&state, &view).is_none());
        assert!(drawn(&layer, &state, &view).is_empty());
        assert_eq!(layer.centers.borrow().entries.capacity(), 0);
    }
}
