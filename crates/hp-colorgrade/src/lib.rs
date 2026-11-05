mod grade;
mod lut;
mod preset;
mod sample;
mod zone;

pub use grade::{Channel, Colorize, Curve, Engine, Grade, Tint};
pub use lut::{
    LutStats, apply_to_rgba, blend_rgba, from_rgba, generate, identity_raw, is_raw, sample_rgb, stats, to_rgba,
};
pub use preset::{PRESETS, Preset, preset};
pub use sample::sample_image;
pub use zone::{Contribution, Fader, Kind, Zone, ZoneReport, resolve, zone_filename};

pub const LUT_SIZE: usize = 32;
pub const RAW_BYTES: usize = LUT_SIZE * LUT_SIZE * LUT_SIZE * 3;
pub const RGBA_BYTES: usize = LUT_SIZE * LUT_SIZE * LUT_SIZE * 4;
pub const DEFAULT_FOLDER: &str = "scripts/colorcorrection";
pub const FOLDERS: [&str; 3] = ["scripts/colorcorrection", "materials/correction", "materials/colorcorrection"];
