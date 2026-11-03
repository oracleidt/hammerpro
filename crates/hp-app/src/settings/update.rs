use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    pub check_on_startup: bool,
    pub show_dialog: bool,
    pub skipped_version: String,
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self { check_on_startup: true, show_dialog: true, skipped_version: String::new() }
    }
}

impl UpdateSettings {
    pub fn skips(&self, tag: &str) -> bool {
        let skipped = self.skipped_version.trim();
        !skipped.is_empty() && skipped.trim_start_matches(['v', 'V']) == tag.trim().trim_start_matches(['v', 'V'])
    }

    pub fn announces(&self, tag: &str) -> bool {
        self.show_dialog && !self.skips(tag)
    }

    pub fn announce_everything(&mut self) {
        self.show_dialog = true;
        self.skipped_version.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_check_on_startup_and_skip_nothing() {
        let s = UpdateSettings::default();
        assert!(s.check_on_startup);
        assert!(!s.skips("v1.0.3"));
    }

    #[test]
    fn skipping_compares_versions_not_prefixes() {
        let s = UpdateSettings { skipped_version: "v1.0.3".into(), ..UpdateSettings::default() };
        assert!(s.skips("v1.0.3"));
        assert!(s.skips("1.0.3"));
        assert!(!s.skips("v1.0.4"));
        assert!(!s.skips("v1.0.30"));
    }

    #[test]
    fn survives_a_toml_round_trip_and_old_files() {
        let s = UpdateSettings { check_on_startup: false, show_dialog: false, skipped_version: "v2.0.0".into() };
        let text = toml::to_string(&s).unwrap();
        let back: UpdateSettings = toml::from_str(&text).unwrap();
        assert_eq!(back, s);
        let old: UpdateSettings = toml::from_str("").unwrap();
        assert!(old.check_on_startup && old.show_dialog && old.skipped_version.is_empty());
        let partial: UpdateSettings = toml::from_str("skipped_version = \"v1.0.9\"").unwrap();
        assert!(partial.check_on_startup && partial.show_dialog);
        assert_eq!(partial.skipped_version, "v1.0.9");
    }

    #[test]
    fn the_window_is_announced_unless_it_is_turned_off_or_the_version_is_skipped() {
        let mut s = UpdateSettings::default();
        assert!(s.announces("v1.0.4"));
        s.skipped_version = "v1.0.4".into();
        assert!(!s.announces("v1.0.4"));
        assert!(s.announces("v1.0.5"));
        s.show_dialog = false;
        assert!(!s.announces("v1.0.5"));
    }

    #[test]
    fn announcing_everything_again_clears_both_ways_of_hiding_it() {
        let mut s = UpdateSettings { check_on_startup: false, show_dialog: false, skipped_version: "v1.0.4".into() };
        s.announce_everything();
        assert!(s.announces("v1.0.4"));
        assert!(s.skipped_version.is_empty());
        assert!(!s.check_on_startup);
    }
}
