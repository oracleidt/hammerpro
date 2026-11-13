use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MeshSettings {
    pub uv_lock: bool,
    pub live_preview: bool,
    pub four_way_blends: bool,
    pub disp_selection_opacity: f32,
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
            disp_selection_opacity: 0.3,
            noise_min: -4.0,
            noise_max: 4.0,
            mesh_export_meters: false,
            mesh_export_z_up: false,
            disp_tool: BTreeMap::new(),
        }
    }
}

impl MeshSettings {
    pub fn displacement_selection_opacity(&self) -> f32 {
        if self.disp_selection_opacity.is_finite() { self.disp_selection_opacity.clamp(0.0, 1.0) } else { 0.3 }
    }

    pub fn noise_range(&self) -> (f64, f64) {
        let fix = |v: f64| if v.is_finite() { v.clamp(-1024.0, 1024.0) } else { 0.0 };
        let (a, b) = (fix(self.noise_min), fix(self.noise_max));
        (a.min(b), a.max(b))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergePrompt {
    #[default]
    Off,
    Ask,
    Always,
    Never,
}

impl MergePrompt {
    pub const ALL: [MergePrompt; 4] = [MergePrompt::Off, MergePrompt::Ask, MergePrompt::Always, MergePrompt::Never];

    pub fn name(self) -> &'static str {
        match self {
            MergePrompt::Off => "off",
            MergePrompt::Ask => "ask",
            MergePrompt::Always => "always",
            MergePrompt::Never => "never",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name().eq_ignore_ascii_case(text.trim()))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VertexSettings {
    pub merge_prompt: MergePrompt,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displacement_opacity_survives_settings_roundtrip_and_old_files_keep_the_default() {
        let old: MeshSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.displacement_selection_opacity(), 0.3);
        let mut settings = MeshSettings { disp_selection_opacity: 0.237, ..Default::default() };
        let saved = serde_json::to_string(&settings).unwrap();
        let restored: MeshSettings = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.displacement_selection_opacity(), 0.237);
        for (value, expected) in [(f32::NAN, 0.3), (-1.0, 0.0), (2.0, 1.0)] {
            settings.disp_selection_opacity = value;
            assert_eq!(settings.displacement_selection_opacity(), expected);
        }
    }

    #[test]
    fn vertex_merge_prompt_defaults_to_off_and_roundtrips() {
        let old: VertexSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(old.merge_prompt, MergePrompt::Off);
        assert_eq!(VertexSettings::default().merge_prompt, MergePrompt::Off);
        for m in MergePrompt::ALL {
            let saved = serde_json::to_string(&VertexSettings { merge_prompt: m }).unwrap();
            assert!(saved.contains(m.name()), "{saved}");
            assert_eq!(serde_json::from_str::<VertexSettings>(&saved).unwrap().merge_prompt, m);
            assert_eq!(MergePrompt::parse(&m.name().to_uppercase()), Some(m));
        }
        assert_eq!(MergePrompt::parse("maybe"), None);
    }
}
