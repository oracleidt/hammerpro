use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlacementSettings {
    pub show_prefabs: bool,
    pub prefab_library: Option<PathBuf>,
    pub prefab_dirs: Vec<PathBuf>,
    pub game_prefabs: bool,
    pub prefab_tile: f32,
    pub group_prefabs: bool,
    pub sprinkle: SprinkleSettings,
    pub auto_settle_on_create: bool,
    pub show_real_dimensions: bool,
    pub problem_checks: bool,
    pub overlap_check: bool,
}

impl Default for PlacementSettings {
    fn default() -> Self {
        Self {
            show_prefabs: true,
            prefab_library: None,
            prefab_dirs: Vec::new(),
            game_prefabs: true,
            prefab_tile: PlacementSettings::DEFAULT_TILE,
            group_prefabs: true,
            sprinkle: SprinkleSettings::default(),
            auto_settle_on_create: false,
            show_real_dimensions: false,
            problem_checks: true,
            overlap_check: false,
        }
    }
}

impl PlacementSettings {
    pub const DEFAULT_TILE: f32 = 96.0;
    pub const TILE_RANGE: std::ops::RangeInclusive<f32> = 56.0..=192.0;

    pub fn library_dir(&self) -> PathBuf {
        match &self.prefab_library {
            Some(p) if !p.as_os_str().is_empty() => p.clone(),
            _ => crate::settings::Settings::config_dir().join("prefabs"),
        }
    }

    pub fn tile_size(&self) -> f32 {
        if self.prefab_tile.is_finite() {
            self.prefab_tile.clamp(*Self::TILE_RANGE.start(), *Self::TILE_RANGE.end())
        } else {
            Self::DEFAULT_TILE
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SprinkleMode {
    #[default]
    Add,
    Subtract,
    Replace,
    Overwrite,
}

impl SprinkleMode {
    pub const ALL: [SprinkleMode; 4] =
        [SprinkleMode::Add, SprinkleMode::Subtract, SprinkleMode::Replace, SprinkleMode::Overwrite];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SprinkleSettings {
    pub kind: String,
    pub mode: SprinkleMode,
    pub density: f32,
    pub radius: f64,
    pub use_grid: bool,
    pub random_yaw: bool,
    pub random_offset: f32,
    pub z_offset: f64,
    pub align_to_normal: bool,
    pub extra_dirs: Vec<PathBuf>,
}

impl Default for SprinkleSettings {
    fn default() -> Self {
        Self {
            kind: String::new(),
            mode: SprinkleMode::Add,
            density: 50.0,
            radius: 128.0,
            use_grid: false,
            random_yaw: true,
            random_offset: 50.0,
            z_offset: 0.0,
            align_to_normal: false,
            extra_dirs: Vec::new(),
        }
    }
}

impl SprinkleSettings {
    pub const RADIUS_RANGE: std::ops::RangeInclusive<f64> = 8.0..=4096.0;

    pub fn radius(&self) -> f64 {
        if self.radius.is_finite() {
            self.radius.clamp(*Self::RADIUS_RANGE.start(), *Self::RADIUS_RANGE.end())
        } else {
            128.0
        }
    }

    pub fn chance(&self) -> f64 {
        if self.density.is_finite() { f64::from(self.density.clamp(0.0, 100.0)) / 100.0 } else { 0.5 }
    }

    pub fn jitter(&self) -> f64 {
        if self.random_offset.is_finite() { f64::from(self.random_offset.clamp(0.0, 100.0)) / 100.0 } else { 0.0 }
    }

    pub fn z_offset(&self) -> f64 {
        if self.z_offset.is_finite() { self.z_offset.clamp(-4096.0, 4096.0) } else { 0.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_stay_out_of_the_file_and_round_trip() {
        let s = PlacementSettings::default();
        let text = toml::to_string(&s).expect("serialize");
        let back: PlacementSettings = toml::from_str(&text).expect("parse");
        assert_eq!(back, s);
        let partial: PlacementSettings =
            toml::from_str("show_prefabs = false\n[sprinkle]\nmode = \"Overwrite\"\n").expect("partial");
        assert!(!partial.show_prefabs);
        assert_eq!(partial.sprinkle.mode, SprinkleMode::Overwrite);
        assert_eq!(partial.sprinkle.radius, 128.0);
    }

    #[test]
    fn values_are_clamped() {
        let s = SprinkleSettings { radius: f64::NAN, density: 250.0, random_offset: -3.0, ..Default::default() };
        assert_eq!(s.radius(), 128.0);
        assert_eq!(s.chance(), 1.0);
        assert_eq!(s.jitter(), 0.0);
        let s = SprinkleSettings { radius: 1.0e9, z_offset: f64::INFINITY, ..Default::default() };
        assert_eq!(s.radius(), 4096.0);
        assert_eq!(s.z_offset(), 0.0);
        let p = PlacementSettings { prefab_tile: 1000.0, ..Default::default() };
        assert_eq!(p.tile_size(), 192.0);
        assert!(p.library_dir().ends_with("prefabs"));
    }
}
