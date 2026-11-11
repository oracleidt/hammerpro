use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackpadMode {
    #[default]
    Auto,
    On,
    Off,
}

impl TrackpadMode {
    pub const ALL: [TrackpadMode; 3] = [TrackpadMode::Auto, TrackpadMode::On, TrackpadMode::Off];

    pub fn active(self, detected: bool) -> bool {
        match self {
            TrackpadMode::Auto => detected,
            TrackpadMode::On => true,
            TrackpadMode::Off => false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackpadScroll3d {
    #[default]
    Orbit,
    Move,
}

impl TrackpadScroll3d {
    pub const ALL: [TrackpadScroll3d; 2] = [TrackpadScroll3d::Orbit, TrackpadScroll3d::Move];
}

pub const TRACKPAD_SPEED_MIN: f32 = 0.25;
pub const TRACKPAD_SPEED_MAX: f32 = 4.0;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InputSettings {
    pub trackpad_mode: TrackpadMode,
    pub trackpad_scroll_3d: TrackpadScroll3d,
    pub trackpad_pan_speed: f32,
    pub trackpad_zoom_speed: f32,
    pub trackpad_orbit_speed: f32,
}

impl Default for InputSettings {
    fn default() -> Self {
        Self {
            trackpad_mode: TrackpadMode::Auto,
            trackpad_scroll_3d: TrackpadScroll3d::Orbit,
            trackpad_pan_speed: 1.0,
            trackpad_zoom_speed: 1.0,
            trackpad_orbit_speed: 1.0,
        }
    }
}

fn speed(v: f32) -> f32 {
    if v.is_finite() { v.clamp(TRACKPAD_SPEED_MIN, TRACKPAD_SPEED_MAX) } else { 1.0 }
}

impl InputSettings {
    pub fn pan_speed(&self) -> f32 {
        speed(self.trackpad_pan_speed)
    }

    pub fn zoom_speed(&self) -> f32 {
        speed(self.trackpad_zoom_speed)
    }

    pub fn orbit_speed(&self) -> f32 {
        speed(self.trackpad_orbit_speed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_and_speeds() {
        assert!(TrackpadMode::Auto.active(true));
        assert!(!TrackpadMode::Auto.active(false));
        assert!(TrackpadMode::On.active(false));
        assert!(!TrackpadMode::Off.active(true));
        let mut s = InputSettings::default();
        assert_eq!(s.trackpad_mode, TrackpadMode::Auto);
        s.trackpad_pan_speed = f32::NAN;
        s.trackpad_zoom_speed = 100.0;
        s.trackpad_orbit_speed = 0.0;
        assert_eq!((s.pan_speed(), s.zoom_speed(), s.orbit_speed()), (1.0, TRACKPAD_SPEED_MAX, TRACKPAD_SPEED_MIN));
    }

    #[test]
    fn old_settings_files_get_the_defaults() {
        let s: InputSettings = toml::from_str("trackpad_mode = \"On\"").unwrap();
        assert_eq!(s.trackpad_mode, TrackpadMode::On);
        assert_eq!(s.trackpad_scroll_3d, TrackpadScroll3d::Orbit);
        let text = toml::to_string(&InputSettings::default()).unwrap();
        assert!(text.contains("trackpad_mode = \"Auto\""), "{text}");
    }
}
