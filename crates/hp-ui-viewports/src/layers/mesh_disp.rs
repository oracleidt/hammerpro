use hp_app::AppState;
use hp_app::view::ViewInfo;
use hp_render::Overlay;

use super::OverlayLayer;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(MeshDispLayer)]
}

pub struct MeshDispLayer;

impl OverlayLayer for MeshDispLayer {
    fn name(&self) -> &'static str {
        "mesh_disp_preview"
    }

    fn add(&self, state: &AppState, _view: &ViewInfo, out: &mut Overlay) {
        if let Some(lines) = &state.runtime.mesh_disp.preview {
            out.lines_on_top.extend(lines.iter().copied());
        }
    }
}
