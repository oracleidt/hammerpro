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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlenderSettings {
    pub path: String,
    pub format: BlenderFormat,
    pub live_link: bool,
}

impl Default for BlenderSettings {
    fn default() -> Self {
        Self { path: String::new(), format: BlenderFormat::Gltf, live_link: true }
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
}
