use glam::DVec3;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EdgeRequest {
    pub seeds: Vec<(u32, DVec3, DVec3)>,
    pub toward: Option<DVec3>,
    pub additive: bool,
    pub exact: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErosionDialog {
    Thermal,
    Hydraulic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPreview {
    pub version: u64,
    pub min: [f64; 2],
    pub size: [f64; 2],
    pub nx: usize,
    pub ny: usize,
    pub heights: Vec<f32>,
}

#[derive(Debug, Default)]
pub struct MeshRuntime {
    pub noise_dialog: bool,
    pub erosion_dialog: Option<ErosionDialog>,
    pub terrain_preview: Option<TerrainPreview>,
    pub edge_request: Option<EdgeRequest>,
    pub disp_loop_request: Option<DispLoopRequest>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DispLoopRequest {
    pub solid: u32,
    pub side: usize,
    pub base_a: DVec3,
    pub base_b: DVec3,
}
