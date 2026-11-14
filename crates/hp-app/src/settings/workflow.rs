use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::actions::KeymapPreset;
use crate::widgets::{PLAIN_COORD_LIMIT, fmt_coord};

pub const GRID_MIN: f64 = 0.01;
pub const GRID_MAX: f64 = 4096.0;
pub const TOOL_GRID_MIN: f64 = 1.0;
pub const TOOL_GRID_MAX: f64 = 1024.0;
pub const GRID_STEP_MIN: f64 = 1.25;
pub const GRID_STEP_MAX: f64 = 10.0;
pub const FLY_RESPONSE_MAX: f32 = 0.5;
pub const FLY_RESPONSE_DEFAULT: f32 = 0.07;
pub const MAX_LAYOUTS: usize = 32;
pub const MAX_LAYOUT_NAME: usize = 48;

pub fn clamp_grid(size: f64, fallback: f64) -> f64 {
    if size.is_finite() && size > 0.0 { size.clamp(GRID_MIN, GRID_MAX) } else { fallback.clamp(GRID_MIN, GRID_MAX) }
}

pub fn tool_grid(step: f64) -> f64 {
    if step.is_finite() { step.clamp(TOOL_GRID_MIN, TOOL_GRID_MAX) } else { TOOL_GRID_MIN }
}

pub fn grid_step(current: f64, factor: f64, larger: bool) -> f64 {
    let factor = if factor.is_finite() { factor.clamp(GRID_STEP_MIN, GRID_STEP_MAX) } else { 2.0 };
    let current = clamp_grid(current, 16.0);
    if factor == 2.0 {
        let ladder = (-3..=12).map(|e| 2f64.powi(e)).filter(|g| *g <= GRID_MAX);
        let next = if larger {
            ladder.into_iter().find(|g| *g > current * (1.0 + 1e-9))
        } else {
            ladder.into_iter().rev().find(|g| *g < current * (1.0 - 1e-9))
        };
        return next.unwrap_or(if larger { GRID_MAX } else { current.min(0.125) });
    }
    let next = if larger { current * factor } else { current / factor };
    let rounded = next.round();
    let next = if rounded >= 1.0 && (next - rounded).abs() < 1e-6 * rounded { rounded } else { next };
    clamp_grid(next, current)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NavScheme {
    #[default]
    HammerPro,
    Hammer,
    HammerPlusPlus,
}

impl NavScheme {
    pub const ALL: [NavScheme; 3] = [NavScheme::HammerPro, NavScheme::Hammer, NavScheme::HammerPlusPlus];

    pub fn hammer_gestures(self) -> bool {
        self.binding(super::NavInput::LeftRight3d).is_some()
    }

    pub fn middle_looks(self) -> bool {
        self.binding(super::NavInput::MiddleDrag3d) == Some(super::NavEffect::LookFly)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CtrlWheel {
    #[default]
    GridSize,
    ZoomAll2d,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DisplayUnits {
    #[default]
    Units,
    Inches,
    FeetInches,
    Meters,
}

impl DisplayUnits {
    pub const ALL: [DisplayUnits; 4] =
        [DisplayUnits::Units, DisplayUnits::Inches, DisplayUnits::FeetInches, DisplayUnits::Meters];

    pub fn fmt_length(self, units: f64) -> String {
        if !units.is_finite() || units.abs() > PLAIN_COORD_LIMIT {
            return fmt_coord(units);
        }
        match self {
            DisplayUnits::Units => fmt_coord(units),
            DisplayUnits::Inches => format!("{}\"", fmt_coord(units)),
            DisplayUnits::FeetInches => {
                let hundredths = (units.abs() * 100.0).round() as i64;
                let sign = if units < 0.0 && hundredths > 0 { "-" } else { "" };
                let feet = hundredths / 1200;
                let inches = fmt_coord((hundredths % 1200) as f64 / 100.0);
                if feet == 0 { format!("{sign}{inches}\"") } else { format!("{sign}{feet}' {inches}\"") }
            }
            DisplayUnits::Meters => {
                let text = format!("{:.2}", units * 0.0254);
                let text = if text == "-0.00" { "0.00".to_owned() } else { text };
                format!("{text} m")
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedLayout {
    pub quadrants: [u8; 4],
    pub split_x: f32,
    pub split_y: f32,
    pub maximized: Option<usize>,
    pub panels: [bool; 6],
}

impl Default for SavedLayout {
    fn default() -> Self {
        Self { quadrants: [0, 1, 2, 3], split_x: 0.5, split_y: 0.5, maximized: None, panels: [true; 6] }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkflowSettings {
    pub keymap_preset: KeymapPreset,
    pub nav_scheme: NavScheme,
    pub invert_mouse_x: bool,
    pub fly_response: f32,
    pub camera_smoothing: bool,
    pub grid_step_factor: f64,
    pub ctrl_wheel: CtrlWheel,
    pub link_2d_zoom: bool,
    pub drag_autopan: bool,
    pub units: DisplayUnits,
    pub show_view_info: bool,
    pub show_bounds: bool,
    pub show_toolbar: bool,
    pub show_tool_palette: bool,
    pub tool_palette_columns: u8,
    pub toolbar_rows: u8,
    pub toolbar_order: Vec<String>,
    pub toolbar_hidden: Vec<String>,
    pub show_status_bar: bool,
    pub show_history: bool,
    pub layouts: BTreeMap<String, SavedLayout>,
    pub open_in_new_tab: bool,
    pub team_live_edits: bool,
    pub team_share_scripts: bool,
}

impl Default for WorkflowSettings {
    fn default() -> Self {
        Self {
            keymap_preset: KeymapPreset::HammerPro,
            nav_scheme: NavScheme::HammerPro,
            invert_mouse_x: false,
            fly_response: FLY_RESPONSE_DEFAULT,
            camera_smoothing: true,
            grid_step_factor: 2.0,
            ctrl_wheel: CtrlWheel::GridSize,
            link_2d_zoom: false,
            drag_autopan: false,
            units: DisplayUnits::Units,
            show_view_info: false,
            show_bounds: false,
            show_toolbar: true,
            show_tool_palette: true,
            tool_palette_columns: 1,
            toolbar_rows: 1,
            toolbar_order: Vec::new(),
            toolbar_hidden: Vec::new(),
            show_status_bar: true,
            show_history: false,
            layouts: BTreeMap::new(),
            open_in_new_tab: true,
            team_live_edits: true,
            team_share_scripts: true,
        }
    }
}

impl WorkflowSettings {
    pub fn tool_palette_two_columns(&self) -> bool {
        self.tool_palette_columns >= 2
    }

    pub fn toolbar_two_rows(&self) -> bool {
        self.toolbar_rows >= 2
    }

    pub fn reset_toolbar(&mut self) {
        let d = Self::default();
        self.toolbar_rows = d.toolbar_rows;
        self.toolbar_order = d.toolbar_order;
        self.toolbar_hidden = d.toolbar_hidden;
    }

    pub fn fly_response_secs(&self) -> f64 {
        if !self.camera_smoothing {
            return 0.0;
        }
        let r = self.fly_response;
        f64::from(if r.is_finite() { r.clamp(0.0, FLY_RESPONSE_MAX) } else { FLY_RESPONSE_DEFAULT })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_steps_follow_the_factor() {
        assert_eq!(grid_step(16.0, 2.0, true), 32.0);
        assert_eq!(grid_step(16.0, 2.0, false), 8.0);
        assert_eq!(grid_step(4096.0, 2.0, true), GRID_MAX);
        assert_eq!(grid_step(0.015, 2.0, false), 0.015, "below the ladder: stays");
        assert_eq!(grid_step(0.01, 2.0, true), 0.125);
        assert_eq!(grid_step(80.0, 2.0, false), 64.0);
        assert_eq!(grid_step(80.0, 2.0, true), 128.0);
        assert_eq!(grid_step(0.125, 2.0, false), 0.125);
        let mut g = 0.125;
        for _ in 0..20 {
            g = grid_step(g, 2.0, true);
            assert_eq!(g.log2().fract(), 0.0, "{g}");
        }
        let mut g = 16.0;
        for _ in 0..5 {
            g = grid_step(g, 1.5, true);
        }
        for _ in 0..5 {
            g = grid_step(g, 1.5, false);
        }
        assert_eq!(g, 16.0);
        assert_eq!(grid_step(f64::NAN, 2.0, true), 32.0);
        assert_eq!(grid_step(16.0, f64::NAN, true), 32.0);
        assert_eq!(grid_step(16.0, 0.0, true), 20.0);
        assert_eq!(clamp_grid(-4.0, 8.0), 8.0);
        assert_eq!(clamp_grid(0.001, 8.0), GRID_MIN);
        assert_eq!(clamp_grid(f64::INFINITY, 1e9), GRID_MAX);
    }

    #[test]
    fn tool_grids_stay_between_1_and_1024_inside_the_setting_range() {
        assert_eq!(tool_grid(0.25), 1.0);
        assert_eq!(tool_grid(16.0), 16.0);
        assert_eq!(tool_grid(4096.0), 1024.0);
        assert_eq!(tool_grid(f64::NAN), 1.0);
        const { assert!(GRID_MIN <= TOOL_GRID_MIN && TOOL_GRID_MAX <= GRID_MAX) };
        assert_eq!(clamp_grid(1.0, 8.0), 1.0);
        assert_eq!(clamp_grid(1024.0, 8.0), 1024.0);
    }

    #[test]
    fn lengths_in_every_unit() {
        assert_eq!(DisplayUnits::Units.fmt_length(128.0), "128");
        assert_eq!(DisplayUnits::Units.fmt_length(32.5), "32.50");
        assert_eq!(DisplayUnits::Inches.fmt_length(128.0), "128\"");
        assert_eq!(DisplayUnits::FeetInches.fmt_length(128.0), "10' 8\"");
        assert_eq!(DisplayUnits::FeetInches.fmt_length(12.0), "1' 0\"");
        assert_eq!(DisplayUnits::FeetInches.fmt_length(8.0), "8\"");
        assert_eq!(DisplayUnits::FeetInches.fmt_length(11.999), "1' 0\"");
        assert_eq!(DisplayUnits::FeetInches.fmt_length(-30.5), "-2' 6.50\"");
        assert_eq!(DisplayUnits::Meters.fmt_length(128.0), "3.25 m");
        assert_eq!(DisplayUnits::Meters.fmt_length(-0.01), "0.00 m");
        for u in DisplayUnits::ALL {
            assert_eq!(u.fmt_length(f64::NAN), "NaN");
            assert_eq!(u.fmt_length(1e300), "1.00e300");
            assert!(!u.fmt_length(i64::MAX as f64).contains("9223372036854775807"));
        }
    }

    #[test]
    fn settings_round_trip_and_defaults() {
        let mut s = WorkflowSettings::default();
        assert_eq!(s.fly_response_secs(), f64::from(FLY_RESPONSE_DEFAULT));
        s.keymap_preset = KeymapPreset::HammerPlusPlus;
        s.nav_scheme = NavScheme::Hammer;
        s.units = DisplayUnits::Meters;
        s.layouts.insert("Wide".into(), SavedLayout { split_x: 0.7, ..SavedLayout::default() });
        let text = toml::to_string_pretty(&s).unwrap();
        let back: WorkflowSettings = toml::from_str(&text).unwrap();
        assert_eq!(back, s);
        let partial: WorkflowSettings = toml::from_str("units = \"Inches\"\n").unwrap();
        assert_eq!(partial.units, DisplayUnits::Inches);
        assert!(partial.show_toolbar);
        assert_eq!(
            (partial.tool_palette_columns, partial.toolbar_rows),
            (1, 1),
            "older files keep one column and one row"
        );
        assert!(partial.toolbar_order.is_empty() && partial.toolbar_hidden.is_empty());
        assert!(partial.open_in_new_tab, "files written before the tabs open maps in tabs");
        s.fly_response = f32::NAN;
        assert_eq!(s.fly_response_secs(), f64::from(FLY_RESPONSE_DEFAULT));
        s.fly_response = 9.0;
        assert_eq!(s.fly_response_secs(), f64::from(FLY_RESPONSE_MAX));
        assert!(NavScheme::Hammer.hammer_gestures() && !NavScheme::Hammer.middle_looks());
        assert!(NavScheme::HammerPlusPlus.hammer_gestures() && NavScheme::HammerPlusPlus.middle_looks());
        assert!(!NavScheme::HammerPro.hammer_gestures());
    }

    #[test]
    fn toolbar_and_palette_layout_round_trip_and_reset() {
        let mut s = WorkflowSettings {
            tool_palette_columns: 2,
            toolbar_rows: 2,
            toolbar_order: vec!["render".into(), "file".into()],
            toolbar_hidden: vec!["cordon".into(), "toggle_grid".into()],
            ..WorkflowSettings::default()
        };
        assert!(s.tool_palette_two_columns() && s.toolbar_two_rows());
        let back: WorkflowSettings = toml::from_str(&toml::to_string_pretty(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        s.reset_toolbar();
        assert!(!s.toolbar_two_rows() && s.toolbar_order.is_empty() && s.toolbar_hidden.is_empty());
        assert!(s.tool_palette_two_columns(), "the palette is not part of the toolbar");
    }

    #[test]
    fn disabling_camera_smoothing_preserves_response_and_survives_save() {
        let mut settings =
            WorkflowSettings { camera_smoothing: false, fly_response: 0.35, ..WorkflowSettings::default() };
        assert_eq!(settings.fly_response_secs(), 0.0);
        let saved = toml::to_string(&settings).unwrap();
        settings = toml::from_str(&saved).unwrap();
        assert_eq!(settings.fly_response_secs(), 0.0);
        settings.camera_smoothing = true;
        assert_eq!(settings.fly_response_secs(), f64::from(0.35_f32));
        assert!(toml::from_str::<WorkflowSettings>("").unwrap().camera_smoothing);
    }
}
