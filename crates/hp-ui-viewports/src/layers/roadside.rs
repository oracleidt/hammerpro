use hp_app::AppState;
use hp_app::view::ViewInfo;
use hp_render::{Overlay, OverlayLine, OverlayTri};

use super::OverlayLayer;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(RoadsideLayer)]
}

pub struct RoadsideLayer;

impl OverlayLayer for RoadsideLayer {
    fn name(&self) -> &'static str {
        "roadside"
    }

    fn retained_version(&self, state: &AppState, view: &ViewInfo) -> Option<u64> {
        let r = &state.runtime.roadside;
        if r.preview.is_empty() {
            return None;
        }
        Some(r.revision.wrapping_mul(2).wrapping_add(u64::from(view.is_2d())))
    }

    fn retained(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        let preview = &state.runtime.roadside.preview;
        for (a, b, color) in &preview.lines {
            out.lines_on_top.push(OverlayLine { a: *a, b: *b, color: *color });
        }
        if view.is_2d() {
            return;
        }
        out.triangles_on_top.extend(preview.tris.iter().map(|(p, color)| OverlayTri { p: *p, color: *color }));
    }
}

#[cfg(test)]
mod tests {
    use hp_app::ViewKind;
    use hp_app::testing::test_state;

    use super::*;

    fn view(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 0,
            kind,
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
            cam3d: Default::default(),
            cam2d: Default::default(),
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    #[test]
    fn the_preview_is_drawn_in_3d_and_only_its_lines_in_2d() {
        let mut state = test_state();
        assert_eq!(RoadsideLayer.retained_version(&state, &view(ViewKind::Perspective)), None);
        state.runtime.roadside.set_preview(hp_app::runtime::RoadPreview {
            tris: vec![([[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], [1.0; 4])],
            lines: vec![([0.0; 3], [1.0; 3], [1.0; 4])],
            ..Default::default()
        });
        let v3 = RoadsideLayer.retained_version(&state, &view(ViewKind::Perspective)).unwrap();
        let v2 = RoadsideLayer.retained_version(&state, &view(ViewKind::Top)).unwrap();
        assert_ne!(v3, v2);
        let mut out = Overlay::default();
        RoadsideLayer.retained(&state, &view(ViewKind::Perspective), &mut out);
        assert_eq!((out.lines_on_top.len(), out.triangles_on_top.len()), (1, 1));
        let mut flat = Overlay::default();
        RoadsideLayer.retained(&state, &view(ViewKind::Top), &mut flat);
        assert_eq!((flat.lines_on_top.len(), flat.triangles_on_top.len()), (1, 0));
        state.runtime.roadside.clear_preview();
        assert_eq!(RoadsideLayer.retained_version(&state, &view(ViewKind::Top)), None);
    }
}
