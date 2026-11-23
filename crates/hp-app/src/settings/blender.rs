use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlenderFormat {
    #[default]
    Gltf,
    Obj,
    Fbx,
}

impl BlenderFormat {
    pub const ALL: [BlenderFormat; 3] = [BlenderFormat::Gltf, BlenderFormat::Obj, BlenderFormat::Fbx];

    pub fn id(self) -> &'static str {
        match self {
            BlenderFormat::Gltf => "gltf",
            BlenderFormat::Obj => "obj",
            BlenderFormat::Fbx => "fbx",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|f| f.id().eq_ignore_ascii_case(id.trim()) || (*f == Self::Gltf && id.eq_ignore_ascii_case("glb")))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlenderScale {
    #[default]
    Meters,
    SourceUnits,
}

pub const UNITS_PER_METER: f64 = 39.370_078_740_157_48;

impl BlenderScale {
    pub const ALL: [BlenderScale; 2] = [BlenderScale::Meters, BlenderScale::SourceUnits];

    pub fn id(self) -> &'static str {
        match self {
            BlenderScale::Meters => "meters",
            BlenderScale::SourceUnits => "source_units",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        let id = id.trim();
        Self::ALL.into_iter().find(|s| s.id().eq_ignore_ascii_case(id)).or_else(|| {
            match id.to_ascii_lowercase().as_str() {
                "m" | "meter" | "metric" | "0.0254" => Some(BlenderScale::Meters),
                "source" | "units" | "1:1" | "1" => Some(BlenderScale::SourceUnits),
                _ => None,
            }
        })
    }

    pub fn units_per_blender_unit(self) -> f64 {
        match self {
            BlenderScale::Meters => UNITS_PER_METER,
            BlenderScale::SourceUnits => 1.0,
        }
    }

    pub fn blender_units_per_unit(self) -> f64 {
        1.0 / self.units_per_blender_unit()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlenderSettings {
    pub path: String,
    pub format: BlenderFormat,
    pub live_link: bool,
    pub scale: BlenderScale,
}

impl Default for BlenderSettings {
    fn default() -> Self {
        Self { path: String::new(), format: BlenderFormat::Gltf, live_link: true, scale: BlenderScale::Meters }
    }
}

impl BlenderSettings {
    pub fn work_dir() -> std::path::PathBuf {
        dirs::data_local_dir().unwrap_or_else(super::Settings::config_dir).join("HammerPro").join("blender")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_format_names() {
        let s = BlenderSettings::default();
        assert!(s.path.is_empty() && s.live_link);
        assert_eq!(s.format, BlenderFormat::Gltf);
        assert_eq!(BlenderFormat::from_id("GLB"), Some(BlenderFormat::Gltf));
        assert_eq!(BlenderFormat::from_id(" fbx "), Some(BlenderFormat::Fbx));
        assert_eq!(BlenderFormat::from_id("stl"), None);
        assert!(BlenderSettings::work_dir().ends_with("blender"));
    }

    #[test]
    fn scale_defaults_to_meters_and_reads_old_settings() {
        assert_eq!(BlenderSettings::default().scale, BlenderScale::Meters);
        let old: BlenderSettings = serde_json::from_str(r#"{"path":"","format":"obj","live_link":false}"#).unwrap();
        assert_eq!(old.scale, BlenderScale::Meters);
        let one: BlenderSettings = serde_json::from_str(r#"{"scale":"source_units"}"#).unwrap();
        assert_eq!(one.scale, BlenderScale::SourceUnits);
        assert_eq!(BlenderScale::from_id("1:1"), Some(BlenderScale::SourceUnits));
        assert_eq!(BlenderScale::from_id(" Meters "), Some(BlenderScale::Meters));
        assert_eq!(BlenderScale::from_id("feet"), None);
        assert!((BlenderScale::Meters.blender_units_per_unit() - 0.0254).abs() < 1e-12);
        assert_eq!(BlenderScale::SourceUnits.units_per_blender_unit(), 1.0);
    }
}
