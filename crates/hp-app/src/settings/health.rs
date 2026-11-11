use hp_editor::limits::LimitProfile;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HealthSettings {
    pub live_checks: bool,
    pub live_leak_check: bool,
    pub show_leak_path: bool,
    pub limit_profile: String,
    pub visible_only: bool,
    pub portal_width: f32,
    pub leaf_grid: bool,
    pub leaf_width: f32,
    pub offer_portals: bool,
    pub portal_heat: bool,
    pub portal_xray: bool,
    pub warn_duplicates: bool,
}

impl Default for HealthSettings {
    fn default() -> Self {
        Self {
            live_checks: true,
            live_leak_check: true,
            show_leak_path: true,
            limit_profile: "auto".to_owned(),
            visible_only: false,
            portal_width: 1.0,
            leaf_grid: false,
            leaf_width: 1.0,
            offer_portals: true,
            portal_heat: false,
            portal_xray: true,
            warn_duplicates: true,
        }
    }
}

impl HealthSettings {
    pub fn profile(&self) -> Option<LimitProfile> {
        LimitProfile::ALL.into_iter().find(|p| p.id().eq_ignore_ascii_case(self.limit_profile.trim()))
    }

    pub fn set_profile(&mut self, profile: Option<LimitProfile>) {
        self.limit_profile = profile.map_or("auto", LimitProfile::id).to_owned();
    }

    pub fn portal_width_px(&self) -> f32 {
        if self.portal_width.is_finite() { self.portal_width.clamp(1.0, 6.0) } else { 1.0 }
    }

    pub fn leaf_width_px(&self) -> f32 {
        if self.leaf_width.is_finite() { self.leaf_width.clamp(1.0, 6.0) } else { 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_load_with_their_defaults() {
        let s: HealthSettings = toml::from_str("live_leak_check = false\nlimit_profile = \"plusplus\"").expect("parse");
        assert_eq!(
            s,
            HealthSettings { live_leak_check: false, limit_profile: "plusplus".into(), ..HealthSettings::default() }
        );
        assert!(s.live_checks);
        assert!(s.warn_duplicates);
        assert_eq!(s.profile(), Some(LimitProfile::PlusPlus));
        let back: HealthSettings = toml::from_str(&toml::to_string(&s).expect("write")).expect("read");
        assert_eq!(back, s);
        let mut d = HealthSettings::default();
        assert_eq!(d.profile(), None);
        d.set_profile(Some(LimitProfile::Stock));
        assert_eq!(d.limit_profile, "stock");
        d.portal_width = f32::NAN;
        assert_eq!(d.portal_width_px(), 1.0);
        d.portal_width = 9.0;
        assert_eq!(d.portal_width_px(), 6.0);
    }
}
