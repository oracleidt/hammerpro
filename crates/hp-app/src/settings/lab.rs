use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LabSettings {
    pub show_nav_mesh: bool,
    pub nav_connections: bool,
    pub nav_fill: bool,
    pub show_particles: bool,
    pub particles_selected_only: bool,
    pub show_entity_fx: bool,
    pub unprop_detail: bool,
    pub unprop_keep_prop: bool,
    pub unprop_shape: UnpropShape,
    pub model_lod: ModelLodSettings,
    pub propper_physics: bool,
    pub propper_static: bool,
    pub propper_collision: bool,
    pub propper_mass: f32,
    pub model_keep_params: bool,
    pub color_correction_preview: bool,
    pub combine_origin: CombineOrigin,
    pub combine_keep_originals: bool,
    pub combine_collision: bool,
    pub combine_max_pieces: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelLodSettings {
    pub enabled: bool,
    pub levels: u32,
    pub reduction: f32,
    #[serde(serialize_with = "serialize_lod_distances", deserialize_with = "deserialize_lod_distances")]
    pub distances: [Option<f64>; 6],
}

fn serialize_lod_distances<S: serde::Serializer>(values: &[Option<f64>; 6], serializer: S) -> Result<S::Ok, S::Error> {
    values.map(|value| value.unwrap_or(0.0)).serialize(serializer)
}

fn deserialize_lod_distances<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<[Option<f64>; 6], D::Error> {
    let values = <[Option<f64>; 6]>::deserialize(deserializer)?;
    Ok(values.map(|value| value.filter(|value| value.is_finite() && *value > 0.0)))
}

impl ModelLodSettings {
    pub const MIN_LEVELS: u32 = 3;
    pub const MAX_LEVELS: u32 = 6;

    pub fn clamped(self) -> Self {
        Self {
            enabled: self.enabled,
            levels: self.levels.clamp(Self::MIN_LEVELS, Self::MAX_LEVELS),
            reduction: if self.reduction.is_finite() { self.reduction.clamp(0.2, 0.9) } else { 0.5 },
            distances: self
                .distances
                .map(|value| value.filter(|v| v.is_finite() && *v > 0.0).map(|v| v.min(1_000_000.0))),
        }
    }
}

impl Default for ModelLodSettings {
    fn default() -> Self {
        Self { enabled: true, levels: 4, reduction: 0.5, distances: [None; 6] }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombineOrigin {
    #[default]
    Center,
    Bottom,
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
            show_entity_fx: true,
            unprop_detail: true,
            unprop_keep_prop: false,
            unprop_shape: UnpropShape::Auto,
            model_lod: ModelLodSettings::default(),
            propper_physics: false,
            propper_static: true,
            propper_collision: true,
            propper_mass: 10.0,
            model_keep_params: true,
            color_correction_preview: false,
            combine_origin: CombineOrigin::Center,
            combine_keep_originals: true,
            combine_collision: true,
            combine_max_pieces: 32,
        }
    }
}

impl LabSettings {
    pub const MAX_COMBINE_PIECES: u32 = 128;

    pub fn combine_pieces(&self) -> usize {
        self.combine_max_pieces.clamp(1, Self::MAX_COMBINE_PIECES) as usize
    }

    pub fn propper_options(&self) -> hp_editor::prop_export::PropOptions {
        hp_editor::prop_export::PropOptions {
            physics: self.propper_physics,
            static_prop: self.propper_static,
            collision: self.propper_collision,
            mass: f64::from(self.propper_mass),
        }
        .clamped()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_distances_survive_settings_and_older_settings_remain_automatic() {
        let old: ModelLodSettings = serde_json::from_str(r#"{"enabled":true,"levels":4,"reduction":0.5}"#).unwrap();
        assert_eq!(old.distances, [None; 6]);
        let custom = ModelLodSettings { distances: [Some(37.5), None, Some(700.0), None, None, Some(6000.0)], ..old };
        assert_eq!(serde_json::from_str::<ModelLodSettings>(&serde_json::to_string(&custom).unwrap()).unwrap(), custom);
        for settings in [old, custom] {
            let text = toml::to_string(&settings).unwrap();
            assert_eq!(toml::from_str::<ModelLodSettings>(&text).unwrap(), settings);
        }
        let bad = ModelLodSettings {
            distances: [Some(f64::NAN), Some(f64::INFINITY), Some(-1.0), Some(0.0), Some(20.5), Some(2e9)],
            ..old
        }
        .clamped();
        assert_eq!(bad.distances, [None, None, None, None, Some(20.5), Some(1_000_000.0)]);
    }
}
