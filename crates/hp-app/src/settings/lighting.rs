use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LightingQuality {
    #[default]
    Draft,
    Normal,
    Final,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BakePriority {
    Idle,
    #[default]
    BelowNormal,
    Normal,
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
    pub priority: BakePriority,
    pub pause_minimized: bool,
    pub pause_unfocused: bool,
    pub drag_pause_ms: u32,
    pub upload_mb_per_frame: u32,
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
            priority: BakePriority::BelowNormal,
            pause_minimized: true,
            pause_unfocused: false,
            drag_pause_ms: 120,
            upload_mb_per_frame: 4,
        }
    }
}

impl LightingSettings {
    pub const MAX_THREADS: u32 = 256;
    pub const LUXELS_RANGE: std::ops::RangeInclusive<u32> = 1..=64;
    pub const RADIUS_RANGE: std::ops::RangeInclusive<u32> = 256..=32768;
    pub const DRAG_PAUSE_RANGE: std::ops::RangeInclusive<u32> = 0..=2000;
    pub const UPLOAD_RANGE: std::ops::RangeInclusive<u32> = 1..=64;

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
            priority: self.priority,
            pause_minimized: self.pause_minimized,
            pause_unfocused: self.pause_unfocused,
            drag_pause_ms: self.drag_pause_ms.min(*Self::DRAG_PAUSE_RANGE.end()),
            upload_mb_per_frame: self.upload_mb_per_frame.clamp(*Self::UPLOAD_RANGE.start(), *Self::UPLOAD_RANGE.end()),
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
        assert!(d.pause_minimized && !d.pause_unfocused);
        assert_eq!((d.priority, d.drag_pause_ms, d.upload_mb_per_frame), (BakePriority::BelowNormal, 120, 4));
        let odd = LightingSettings { drag_pause_ms: 99_999, upload_mb_per_frame: 0, ..d };
        assert_eq!((odd.sanitized().drag_pause_ms, odd.sanitized().upload_mb_per_frame), (2000, 1));
        let big = LightingSettings { upload_mb_per_frame: 500, ..d };
        assert_eq!(big.sanitized().upload_mb_per_frame, 64);
        let old: LightingSettings = toml::from_str("threads = 3\nprepare = false").unwrap();
        assert_eq!(
            (old.threads, old.prepare, old.priority, old.pause_minimized),
            (3, false, BakePriority::BelowNormal, true)
        );
        let idle: LightingSettings = toml::from_str("priority = \"Idle\"").unwrap();
        assert_eq!(idle.priority, BakePriority::Idle);
        let parsed: LightingSettings = toml::from_str("").unwrap();
        assert_eq!(parsed, d);
    }
}
