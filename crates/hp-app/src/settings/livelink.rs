use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LiveLinkSettings {
    pub camera_to_game: bool,
    pub camera_from_game: bool,
    pub highlight_selection: bool,
    pub light_previews: bool,
    pub load_into_running_game: bool,
    pub remove_addon_on_stop: bool,
}

impl Default for LiveLinkSettings {
    fn default() -> Self {
        Self {
            camera_to_game: false,
            camera_from_game: false,
            highlight_selection: true,
            light_previews: true,
            load_into_running_game: true,
            remove_addon_on_stop: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_sync_is_off_by_default() {
        let s = LiveLinkSettings::default();
        assert!(!s.camera_to_game && !s.camera_from_game && !s.remove_addon_on_stop);
        assert!(s.highlight_selection && s.light_previews && s.load_into_running_game);
        let back: LiveLinkSettings = toml::from_str("camera_to_game = true").unwrap();
        assert!(back.camera_to_game && back.highlight_selection);
    }
}
