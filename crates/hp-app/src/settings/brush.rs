use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RoadPreset {
    pub road: String,
    pub entity_class: Option<String>,
    pub keep_active: bool,
}

impl Default for RoadPreset {
    fn default() -> Self {
        Self { road: String::new(), entity_class: None, keep_active: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrushSettings {
    pub reorient_primitives: bool,
    pub clip_measurements: bool,
    pub clip_current_material: bool,
    pub road_presets: BTreeMap<String, RoadPreset>,
    pub detail: FuncDetailSettings,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DetailSplitSetting {
    #[default]
    Selection,
    Cluster,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FuncDetailSettings {
    pub split: DetailSplitSetting,
    pub check_sealing: bool,
    pub tint: bool,
    pub max_size: f64,
    pub steps: bool,
    pub trims: bool,
    pub pillars: bool,
    pub small: bool,
    pub complex: bool,
    pub in_selection: bool,
}

impl Default for FuncDetailSettings {
    fn default() -> Self {
        Self {
            split: DetailSplitSetting::Selection,
            check_sealing: true,
            tint: false,
            max_size: 1024.0,
            steps: true,
            trims: true,
            pillars: true,
            small: true,
            complex: true,
            in_selection: false,
        }
    }
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            reorient_primitives: true,
            clip_measurements: true,
            clip_current_material: false,
            road_presets: BTreeMap::new(),
            detail: FuncDetailSettings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_load_with_their_defaults() {
        let s: BrushSettings = toml::from_str("clip_measurements = false").expect("parse");
        assert_eq!(s, BrushSettings { clip_measurements: false, ..BrushSettings::default() });
        assert!(s.reorient_primitives);
        let back: BrushSettings = toml::from_str(&toml::to_string(&s).expect("write")).expect("read");
        assert_eq!(back, s);
    }

    #[test]
    fn func_detail_settings_load_from_a_partial_table() {
        let s: BrushSettings = toml::from_str(
            "[detail]
split = \"cluster\"
tint = true",
        )
        .expect("parse");
        assert_eq!(s.detail.split, DetailSplitSetting::Cluster);
        assert!(s.detail.tint && s.detail.check_sealing && s.detail.steps);
        assert_eq!(s.detail.max_size, 1024.0);
        let back: BrushSettings = toml::from_str(&toml::to_string(&s).expect("write")).expect("read");
        assert_eq!(back, s);
    }

    #[test]
    fn named_road_presets_persist_with_unicode_names_and_creation_settings() {
        let mut settings = BrushSettings::default();
        let preset = RoadPreset {
            road: "r1;lanes=1;lw=1024;pts=0,0,0 64,0,0".into(),
            entity_class: Some("func_detail".into()),
            keep_active: false,
        };
        settings.road_presets.insert("Широкая дорога".into(), preset.clone());
        let back: BrushSettings = toml::from_str(&toml::to_string(&settings).expect("write")).expect("read");
        assert_eq!(back.road_presets.get("Широкая дорога"), Some(&preset));
        assert_eq!(back, settings);
        let old: RoadPreset = toml::from_str("road = 'r1;pts=0,0,0 64,0,0'").expect("old preset");
        assert!(old.keep_active && old.entity_class.is_none());
    }
}
