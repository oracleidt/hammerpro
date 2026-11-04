use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub struct MeshDispRuntime {
    pub preview: Option<Arc<Vec<hp_render::OverlayLine>>>,
}
