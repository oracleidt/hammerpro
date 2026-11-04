use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeshSettings {
    pub uv_lock: bool,
    pub live_preview: bool,
    pub four_way_blends: bool,
    pub noise_min: f64,
    pub noise_max: f64,
    pub mesh_export_meters: bool,
    pub mesh_export_z_up: bool,
    pub disp_tool: BTreeMap<String, String>,
}

impl Default for MeshSettings {
    fn default() -> Self {
        Self {
            uv_lock: false,
            live_preview: true,
            four_way_blends: true,
            noise_min: -4.0,
            noise_max: 4.0,
            mesh_export_meters: false,
            mesh_export_z_up: false,
            disp_tool: BTreeMap::new(),
        }
    }
}

impl MeshSettings {
    pub fn noise_range(&self) -> (f64, f64) {
        let fix = |v: f64| if v.is_finite() { v.clamp(-1024.0, 1024.0) } else { 0.0 };
        let (a, b) = (fix(self.noise_min), fix(self.noise_max));
        (a.min(b), a.max(b))
    }
}
