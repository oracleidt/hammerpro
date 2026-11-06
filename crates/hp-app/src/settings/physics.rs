use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhysicsSettings {
    pub gravity: f64,
    pub air_density: f64,
    pub time_scale: f64,
    pub shoot_speed: f64,
    pub explosion_speed: f64,
    pub explosion_radius: f64,
}

impl Default for PhysicsSettings {
    fn default() -> Self {
        Self {
            gravity: 600.0,
            air_density: 2.0,
            time_scale: 1.0,
            shoot_speed: 300.0,
            explosion_speed: 500.0,
            explosion_radius: 256.0,
        }
    }
}
