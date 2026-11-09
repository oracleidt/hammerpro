use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum HelperScope {
    #[default]
    Selected,
    All,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConnectionsMode {
    Off,
    #[default]
    Selected,
    All,
}

impl ConnectionsMode {
    pub fn next(self) -> Self {
        match self {
            ConnectionsMode::Off => ConnectionsMode::Selected,
            ConnectionsMode::Selected => ConnectionsMode::All,
            ConnectionsMode::All => ConnectionsMode::Off,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HelpersSettings {
    pub helper_scope: HelperScope,
    pub connections: ConnectionsMode,
    pub show_overlays: bool,
    pub show_editor_objects: bool,
    pub show_entity_names: bool,
    pub show_brush_centers: bool,
    pub show_ropes: bool,
    pub simulate_ropes: bool,
}

impl Default for HelpersSettings {
    fn default() -> Self {
        Self {
            helper_scope: HelperScope::Selected,
            connections: ConnectionsMode::Selected,
            show_overlays: true,
            show_editor_objects: true,
            show_entity_names: true,
            show_brush_centers: true,
            show_ropes: true,
            simulate_ropes: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_cycle_and_old_files_load() {
        assert_eq!(ConnectionsMode::Off.next(), ConnectionsMode::Selected);
        assert_eq!(ConnectionsMode::Selected.next(), ConnectionsMode::All);
        assert_eq!(ConnectionsMode::All.next(), ConnectionsMode::Off);
        let s: HelpersSettings = toml::from_str("connections = \"All\"\nunknown = 3\n").expect("parse");
        assert_eq!(s.connections, ConnectionsMode::All);
        assert!(s.show_overlays && s.show_entity_names && s.show_brush_centers);
        let disabled = HelpersSettings { show_brush_centers: false, ..s };
        let saved: HelpersSettings = toml::from_str(&toml::to_string(&disabled).expect("write")).expect("read");
        assert!(!saved.show_brush_centers);
        let back: HelpersSettings = toml::from_str(&toml::to_string(&s).expect("write")).expect("read");
        assert_eq!(back, s);
    }
}
