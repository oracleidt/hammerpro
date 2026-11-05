use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VtfSettings {
    pub editor_path: String,
    pub prefer_integrated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_round_trip() {
        let s = VtfSettings::default();
        assert!(s.editor_path.is_empty() && !s.prefer_integrated);
        let text =
            toml::to_string(&VtfSettings { editor_path: "C:/x/VTFEdit.exe".into(), prefer_integrated: true }).unwrap();
        let back: VtfSettings = toml::from_str(&text).unwrap();
        assert_eq!(back.editor_path, "C:/x/VTFEdit.exe");
        assert!(back.prefer_integrated);
    }
}
