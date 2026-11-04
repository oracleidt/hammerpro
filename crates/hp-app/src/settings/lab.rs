use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LabSettings {
    pub show_nav_mesh: bool,
    pub nav_connections: bool,
    pub nav_fill: bool,
    pub show_particles: bool,
    pub particles_selected_only: bool,
    pub unprop_detail: bool,
    pub unprop_keep_prop: bool,
    pub unprop_shape: UnpropShape,
    pub model_lod: ModelLodSettings,
    pub model_keep_params: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelLodSettings {
    pub enabled: bool,
    pub levels: u32,
    pub reduction: f32,
}

impl ModelLodSettings {
    pub const MIN_LEVELS: u32 = 3;
    pub const MAX_LEVELS: u32 = 6;

    pub fn clamped(self) -> Self {
        Self {
            enabled: self.enabled,
            levels: self.levels.clamp(Self::MIN_LEVELS, Self::MAX_LEVELS),
            reduction: if self.reduction.is_finite() { self.reduction.clamp(0.2, 0.9) } else { 0.5 },
        }
    }
}

impl Default for ModelLodSettings {
    fn default() -> Self {
        Self { enabled: true, levels: 4, reduction: 0.5 }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnpropShape {
    #[default]
    Auto,
    Collision,
    Render,
}

impl Default for LabSettings {
    fn default() -> Self {
        Self {
            show_nav_mesh: false,
            nav_connections: true,
            nav_fill: true,
            show_particles: true,
            particles_selected_only: false,
            unprop_detail: true,
            unprop_keep_prop: false,
            unprop_shape: UnpropShape::Auto,
            model_lod: ModelLodSettings::default(),
            model_keep_params: true,
        }
    }
}
