use crate::{Colorize, Grade, Tint};

pub struct Preset {
    pub id: &'static str,
    pub build: fn() -> Grade,
}

pub const PRESETS: &[Preset] = &[
    Preset { id: "neutral", build: Grade::default },
    Preset { id: "black_white", build: || Grade { saturation: 0.0, contrast: 0.1, ..Grade::default() } },
    Preset {
        id: "sepia",
        build: || Grade {
            saturation: 0.0,
            colorize: Colorize { color: [1.0, 0.82, 0.58], amount: 0.9 },
            contrast: 0.05,
            ..Grade::default()
        },
    },
    Preset { id: "warm", build: || Grade { temperature: 0.45, saturation: 1.08, ..Grade::default() } },
    Preset { id: "cool", build: || Grade { temperature: -0.45, saturation: 1.05, ..Grade::default() } },
    Preset { id: "high_contrast", build: || Grade { contrast: 0.45, saturation: 1.12, ..Grade::default() } },
    Preset {
        id: "faded",
        build: || {
            let mut g = Grade { saturation: 0.7, contrast: -0.2, brightness: 0.04, ..Grade::default() };
            g.levels[0].out_black = 0.08;
            g
        },
    },
    Preset {
        id: "night",
        build: || Grade {
            exposure: -0.6,
            saturation: 0.55,
            tint: Tint { color: [0.55, 0.7, 1.0], amount: 0.8 },
            contrast: 0.1,
            ..Grade::default()
        },
    },
    Preset { id: "vivid", build: || Grade { saturation: 1.25, vibrance: 0.5, contrast: 0.15, ..Grade::default() } },
    Preset {
        id: "cinematic",
        build: || {
            let mut g = Grade { contrast: 0.2, saturation: 0.9, shadows: 0.1, ..Grade::default() };
            g.tint = Tint { color: [0.85, 1.0, 1.0], amount: 0.35 };
            g.curves[3].points = vec![[0.0, 0.05], [0.5, 0.5], [1.0, 0.92]];
            g.curves[1].points = vec![[0.0, 0.0], [0.5, 0.55], [1.0, 1.0]];
            g
        },
    },
    Preset {
        id: "horror",
        build: || Grade {
            saturation: 0.25,
            contrast: 0.35,
            exposure: -0.3,
            tint: Tint { color: [0.8, 1.0, 0.85], amount: 0.5 },
            ..Grade::default()
        },
    },
    Preset { id: "invert", build: || Grade { invert: true, ..Grade::default() } },
];

pub fn preset(id: &str) -> Option<Grade> {
    PRESETS.iter().find(|p| p.id == id).map(|p| (p.build)())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generate, stats};

    #[test]
    fn presets_exist_and_differ() {
        assert!(preset("zzz").is_none());
        assert!(stats(&generate(&preset("neutral").unwrap(), None)).unwrap().identity);
        for p in PRESETS.iter().filter(|p| p.id != "neutral") {
            let s = stats(&generate(&(p.build)(), None)).unwrap();
            assert!(!s.identity, "{}", p.id);
        }
    }
}
