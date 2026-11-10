use serde::{Deserialize, Serialize};

const HANDLE_SIZE_RANGE: std::ops::RangeInclusive<f32> = 4.0..=16.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectElement {
    #[default]
    Object,
    Face,
    Edge,
    Vertex,
}

impl SelectElement {
    pub const ALL: [SelectElement; 4] =
        [SelectElement::Object, SelectElement::Face, SelectElement::Edge, SelectElement::Vertex];

    pub fn id(self) -> &'static str {
        match self {
            SelectElement::Object => "object",
            SelectElement::Face => "face",
            SelectElement::Edge => "edge",
            SelectElement::Vertex => "vertex",
        }
    }

    pub fn from_id(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|e| e.id() == s || format!("{}s", e.id()) == s)
    }

    pub fn label_key(self) -> &'static str {
        match self {
            SelectElement::Object => "seltool.element.object",
            SelectElement::Face => "seltool.element.face",
            SelectElement::Edge => "seltool.element.edge",
            SelectElement::Vertex => "seltool.element.vertex",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectionSettings {
    pub clone_keeps_visgroups: bool,
    pub rotation_snap_deg: f64,
    pub free_rotation_with_alt: bool,
    pub handle_size: f32,
    pub shift_arrow_clones: bool,
    pub gizmo_local: bool,
    pub click_to_zoom: bool,
    pub stretch_handles: bool,
    pub box_confirm: bool,
    pub random_yaw_min: f64,
    pub random_yaw_max: f64,
    pub random_yaw_snap: f64,
    pub random_tilt: bool,
    pub random_tilt_deg: f64,
    pub element: SelectElement,
}

impl Default for SelectionSettings {
    fn default() -> Self {
        Self {
            clone_keeps_visgroups: true,
            rotation_snap_deg: 15.0,
            free_rotation_with_alt: false,
            handle_size: 7.0,
            shift_arrow_clones: true,
            gizmo_local: false,
            click_to_zoom: false,
            stretch_handles: true,
            box_confirm: true,
            random_yaw_min: 0.0,
            random_yaw_max: 360.0,
            random_yaw_snap: 0.0,
            random_tilt: false,
            random_tilt_deg: 3.0,
            element: SelectElement::Object,
        }
    }
}

impl SelectionSettings {
    pub fn rotation_step(&self) -> f64 {
        if self.rotation_snap_deg.is_finite() { self.rotation_snap_deg.clamp(1.0, 180.0) } else { 15.0 }
    }

    pub fn random_seed() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64);
        now ^ COUNTER.fetch_add(0x9E37_79B9, Ordering::Relaxed).rotate_left(17) ^ u64::from(std::process::id())
    }

    pub fn random_rotate(&self, seed: u64) -> hp_editor::RandomRotate {
        let num = |v: f64, d: f64| if v.is_finite() { v } else { d };
        let min = num(self.random_yaw_min, 0.0).clamp(-360.0, 360.0);
        let max = num(self.random_yaw_max, 360.0).clamp(min, (min + 360.0).min(720.0));
        let tilt = num(self.random_tilt_deg, 3.0).clamp(0.0, hp_editor::MAX_JITTER_DEG);
        hp_editor::RandomRotate {
            min_yaw: min,
            max_yaw: max,
            snap: num(self.random_yaw_snap, 0.0).clamp(0.0, 180.0),
            jitter: if self.random_tilt { tilt } else { 0.0 },
            seed,
        }
    }

    pub fn handle_points(&self) -> f32 {
        if self.handle_size.is_finite() {
            self.handle_size.clamp(*HANDLE_SIZE_RANGE.start(), *HANDLE_SIZE_RANGE.end())
        } else {
            7.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_load_with_their_defaults() {
        let s: SelectionSettings = toml::from_str("rotation_snap_deg = 5.0").expect("parse");
        assert_eq!(s, SelectionSettings { rotation_snap_deg: 5.0, ..SelectionSettings::default() });
        assert!(s.clone_keeps_visgroups && s.shift_arrow_clones && s.stretch_handles && s.box_confirm);
        let back: SelectionSettings = toml::from_str(&toml::to_string(&s).expect("write")).expect("read");
        assert_eq!(back, s);
        let bad =
            SelectionSettings { rotation_snap_deg: f64::NAN, handle_size: 1000.0, ..SelectionSettings::default() };
        assert_eq!(bad.rotation_step(), 15.0);
        assert_eq!(bad.handle_points(), 16.0);
    }

    #[test]
    fn random_rotate_options() {
        let s = SelectionSettings::default();
        let r = s.random_rotate(9);
        assert_eq!((r.min_yaw, r.max_yaw, r.snap, r.jitter, r.seed), (0.0, 360.0, 0.0, 0.0, 9));
        assert!(r.check().is_ok());
        let s = SelectionSettings {
            random_yaw_min: 90.0,
            random_yaw_max: 10.0,
            random_yaw_snap: f64::NAN,
            random_tilt: true,
            random_tilt_deg: 500.0,
            ..SelectionSettings::default()
        };
        let r = s.random_rotate(1);
        assert_eq!((r.min_yaw, r.max_yaw, r.snap, r.jitter), (90.0, 90.0, 0.0, hp_editor::MAX_JITTER_DEG));
        assert!(r.check().is_ok());
    }
}
