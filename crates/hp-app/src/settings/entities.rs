use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserSettings {
    pub favorites: Vec<String>,
    pub recent_materials: Vec<String>,
    pub hide_tool_textures: bool,
    pub hidden_shaders: Vec<String>,
    pub only_used_in_map: bool,
    pub last_filter: String,
    pub excluded_folders: Vec<String>,
    pub fgd_exclusions: bool,
    pub show_alpha: bool,
    pub show_sound_browser: bool,
    pub show_particle_browser: bool,
    #[serde(rename = "model_grid_view")]
    pub model_grid: bool,
    pub tile_size: f32,
    pub gmod_only: bool,
    pub sound_volume: f32,
    pub sound_autoplay: bool,
    pub model_prop_class: String,
}

impl Default for BrowserSettings {
    fn default() -> Self {
        Self {
            favorites: Vec::new(),
            recent_materials: Vec::new(),
            hide_tool_textures: false,
            hidden_shaders: Vec::new(),
            only_used_in_map: false,
            last_filter: String::new(),
            excluded_folders: Vec::new(),
            fgd_exclusions: true,
            show_alpha: false,
            show_sound_browser: false,
            show_particle_browser: false,
            model_grid: true,
            tile_size: 96.0,
            gmod_only: false,
            sound_volume: 0.8,
            sound_autoplay: true,
            model_prop_class: String::new(),
        }
    }
}

impl BrowserSettings {
    pub fn volume(&self) -> f32 {
        if self.sound_volume.is_finite() { self.sound_volume.clamp(0.0, 1.0) } else { 0.8 }
    }

    pub fn tile(&self) -> f32 {
        if self.tile_size.is_finite() { self.tile_size.clamp(48.0, 256.0) } else { 96.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EntitiesSettings {
    pub io_esc_delimiter: bool,
    pub paste_targetname: bool,
    pub texture_decimals: u32,
    pub texture_shift_step: f64,
    pub texture_scale_step: f64,
    pub crowbar_exe: Option<PathBuf>,
    pub path: PathToolSettings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PathToolSettings {
    pub class: String,
    pub base_name: String,
    pub close_loop: bool,
    pub relink_on_delete: bool,
    pub surface_offset: f64,
}

impl Default for PathToolSettings {
    fn default() -> Self {
        Self {
            class: "path_corner".to_owned(),
            base_name: "path".to_owned(),
            close_loop: false,
            relink_on_delete: true,
            surface_offset: 0.0,
        }
    }
}

impl PathToolSettings {
    pub fn class_name(&self) -> &str {
        match self.class.trim() {
            "" => "path_corner",
            c => c,
        }
    }

    pub fn base(&self) -> &str {
        match self.base_name.trim() {
            "" => "path",
            b => b,
        }
    }

    pub fn offset(&self) -> f64 {
        if self.surface_offset.is_finite() { self.surface_offset.clamp(-1024.0, 1024.0) } else { 0.0 }
    }
}

impl Default for EntitiesSettings {
    fn default() -> Self {
        Self {
            io_esc_delimiter: true,
            paste_targetname: false,
            texture_decimals: 4,
            texture_shift_step: 1.0,
            texture_scale_step: 0.005,
            crowbar_exe: None,
            path: PathToolSettings::default(),
        }
    }
}

impl EntitiesSettings {
    pub fn decimals(&self) -> usize {
        self.texture_decimals.min(6) as usize
    }

    pub fn shift_step(&self) -> f64 {
        positive_or(self.texture_shift_step, 1.0, 64.0)
    }

    pub fn scale_step(&self) -> f64 {
        positive_or(self.texture_scale_step, 0.005, 1.0)
    }
}

fn positive_or(v: f64, fallback: f64, max: f64) -> f64 {
    if v.is_finite() && v > 0.0 { v.min(max) } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_load_with_defaults() {
        let b: BrowserSettings = toml::from_str("favorites = [\"a/b\"]").unwrap_or_default();
        assert_eq!(b.favorites, ["a/b"]);
        assert!(b.fgd_exclusions);
        assert!((b.volume() - 0.8).abs() < 1e-6);
        let e: EntitiesSettings = toml::from_str("").unwrap_or_default();
        assert!(e.io_esc_delimiter);
        assert_eq!(e.decimals(), 4);
        assert_eq!(e.path, PathToolSettings::default());
        let p: PathToolSettings = toml::from_str("close_loop = true").unwrap_or_default();
        assert!(p.close_loop && p.relink_on_delete);
        assert_eq!(p.class_name(), "path_corner");
        let blank = PathToolSettings { class: " ".into(), base_name: String::new(), surface_offset: f64::NAN, ..p };
        assert_eq!((blank.class_name(), blank.base(), blank.offset()), ("path_corner", "path", 0.0));
    }

    #[test]
    fn values_are_sanitized() {
        let b = BrowserSettings { sound_volume: f32::NAN, tile_size: 9999.0, ..BrowserSettings::default() };
        assert!((b.volume() - 0.8).abs() < 1e-6);
        assert_eq!(b.tile(), 256.0);
        let e = EntitiesSettings {
            texture_decimals: 40,
            texture_shift_step: -1.0,
            texture_scale_step: f64::INFINITY,
            ..EntitiesSettings::default()
        };
        assert_eq!(e.decimals(), 6);
        assert_eq!(e.shift_step(), 1.0);
        assert_eq!(e.scale_step(), 0.005);
    }
}
