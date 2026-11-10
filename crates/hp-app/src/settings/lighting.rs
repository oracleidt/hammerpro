use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightingQuality {
    #[default]
    Draft,
    Normal,
    Final,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LightingSettings {
    pub auto_bake: bool,
    pub threads: u32,
    pub max_luxels_millions: u32,
    pub tiny_scale: u32,
    pub budget_styles: u32,
    pub show_budget: bool,
    pub quality: LightingQuality,
    pub auto_scope: bool,
    pub auto_scope_luxels_millions: u32,
    pub auto_scope_radius: u32,
    pub visible_first: bool,
    pub prepare: bool,
}

impl Default for LightingSettings {
    fn default() -> Self {
        Self {
            auto_bake: true,
            threads: 0,
            max_luxels_millions: 16,
            tiny_scale: 4,
            budget_styles: 1,
            show_budget: true,
            quality: LightingQuality::Draft,
            auto_scope: false,
            auto_scope_luxels_millions: 2,
            auto_scope_radius: 3072,
            visible_first: true,
            prepare: true,
        }
    }
}

impl LightingSettings {
    pub const MAX_THREADS: u32 = 256;
    pub const LUXELS_RANGE: std::ops::RangeInclusive<u32> = 1..=64;
    pub const RADIUS_RANGE: std::ops::RangeInclusive<u32> = 256..=32768;

    pub fn sanitized(&self) -> Self {
        Self {
            auto_bake: self.auto_bake,
            threads: self.threads.min(Self::MAX_THREADS),
            max_luxels_millions: self.max_luxels_millions.clamp(*Self::LUXELS_RANGE.start(), *Self::LUXELS_RANGE.end()),
            tiny_scale: self.tiny_scale.clamp(1, 64),
            budget_styles: self.budget_styles.clamp(1, 4),
            show_budget: self.show_budget,
            quality: self.quality,
            auto_scope: self.auto_scope,
            auto_scope_luxels_millions: self
                .auto_scope_luxels_millions
                .clamp(*Self::LUXELS_RANGE.start(), *Self::LUXELS_RANGE.end()),
            auto_scope_radius: self.auto_scope_radius.clamp(*Self::RADIUS_RANGE.start(), *Self::RADIUS_RANGE.end()),
            visible_first: self.visible_first,
            prepare: self.prepare,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_ranges() {
        let d = LightingSettings::default();
        assert_eq!(d.sanitized(), d);
        let bad = LightingSettings { threads: 9999, max_luxels_millions: 0, tiny_scale: 0, budget_styles: 9, ..d };
        let s = bad.sanitized();
        assert_eq!((s.threads, s.max_luxels_millions, s.tiny_scale, s.budget_styles), (256, 1, 1, 4));
        let wild = LightingSettings { auto_scope_luxels_millions: 0, auto_scope_radius: 1, ..d };
        assert_eq!((wild.sanitized().auto_scope_luxels_millions, wild.sanitized().auto_scope_radius), (1, 256));
        assert!(!d.auto_scope);
        assert!(d.prepare && d.visible_first);
        let parsed: LightingSettings = toml::from_str("").unwrap();
        assert_eq!(parsed, d);
    }
}
