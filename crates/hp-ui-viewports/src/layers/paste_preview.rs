use hp_app::AppState;
use hp_app::view::ViewInfo;
use hp_render::{Overlay, OverlayLine};

use super::OverlayLayer;

pub const GHOST_COLOR: [f32; 4] = [0.3, 1.0, 0.55, 0.95];

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(PastePreviewLayer)]
}

pub struct PastePreviewLayer;

impl OverlayLayer for PastePreviewLayer {
    fn name(&self) -> &'static str {
        "paste_preview"
    }

    fn retained_version(&self, state: &AppState, _view: &ViewInfo) -> Option<u64> {
        state.runtime.workflow.paste_preview.as_ref().map(|(serial, _)| *serial)
    }

    fn retained(&self, state: &AppState, _view: &ViewInfo, out: &mut Overlay) {
        let Some((_, preview)) = &state.runtime.workflow.paste_preview else { return };
        out.lines_on_top.extend(preview.lines.iter().map(|[a, b]| OverlayLine {
            a: a.as_vec3().to_array(),
            b: b.as_vec3().to_array(),
            color: GHOST_COLOR,
        }));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egui::{Rect, pos2, vec2};
    use glam::{DVec2, DVec3};
    use hp_app::view::{Camera2d, Camera3d, ViewKind};
    use hp_editor::PastePreview;

    use super::*;

    fn view(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 0,
            kind,
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(801.0, 601.0)),
            cam3d: Camera3d::default(),
            cam2d: Camera2d { center: DVec2::ZERO, zoom: 2.0 },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    #[test]
    fn the_ghost_follows_the_preview_and_disappears_with_it() {
        let mut state = hp_app::testing::test_state();
        let view = view(ViewKind::Perspective);
        let layer = PastePreviewLayer;
        assert_eq!(layer.retained_version(&state, &view), None);
        let preview =
            PastePreview { lines: vec![[DVec3::ZERO, DVec3::X * 64.0]], copies: 1, shown_copies: 1, objects: 1 };
        state.runtime.workflow.paste_preview = Some((7, Arc::new(preview)));
        assert_eq!(layer.retained_version(&state, &view), Some(7));
        let mut out = Overlay::default();
        layer.retained(&state, &view, &mut out);
        assert_eq!(out.lines_on_top.len(), 1);
        state.runtime.workflow.paste_preview = None;
        assert_eq!(layer.retained_version(&state, &view), None);
    }
}
