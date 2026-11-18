pub mod apply;
pub mod grid;
pub mod mask;
pub(crate) mod overlay;
pub mod stamps;
pub mod tr;
pub(crate) mod ui;

use std::sync::Arc;

use glam::DVec3;
use hp_app::tool::ToolOption;

pub use apply::TerraStroke;

pub(crate) const UNDO_TERRAFORM: &str = "Terraform displacement";
pub const WHEEL_DEGREES: f64 = 15.0;
pub use stamps::{StampImage, StampShape};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TerraOp {
    #[default]
    Erode,
    Terrace,
    Noise,
    Stamp,
    Cliff,
    Plateau,
    Clone,
    Mask,
}

impl TerraOp {
    pub const ALL: [TerraOp; 8] = [
        TerraOp::Erode,
        TerraOp::Terrace,
        TerraOp::Noise,
        TerraOp::Stamp,
        TerraOp::Cliff,
        TerraOp::Plateau,
        TerraOp::Clone,
        TerraOp::Mask,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TerraOp::Erode => "erode",
            TerraOp::Terrace => "terrace",
            TerraOp::Noise => "noise",
            TerraOp::Stamp => "stamp",
            TerraOp::Cliff => "cliff",
            TerraOp::Plateau => "plateau",
            TerraOp::Clone => "clone",
            TerraOp::Mask => "mask",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "erosion" => Some(TerraOp::Erode),
            "sharpen" => Some(TerraOp::Cliff),
            _ => TerraOp::ALL.into_iter().find(|o| o.name() == name),
        }
    }

    pub fn shift_smooths(self) -> bool {
        matches!(self, TerraOp::Erode | TerraOp::Terrace | TerraOp::Clone)
    }

    pub fn label_key(self) -> &'static str {
        match self {
            TerraOp::Erode => "tf.op.erode",
            TerraOp::Terrace => "tf.op.terrace",
            TerraOp::Noise => "tf.op.noise",
            TerraOp::Stamp => "tf.op.stamp",
            TerraOp::Cliff => "tf.op.cliff",
            TerraOp::Plateau => "tf.op.plateau",
            TerraOp::Clone => "tf.op.clone",
            TerraOp::Mask => "tf.op.mask",
        }
    }

    pub fn tip_key(self) -> &'static str {
        match self {
            TerraOp::Erode => "tf.op.erode_tip",
            TerraOp::Terrace => "tf.op.terrace_tip",
            TerraOp::Noise => "tf.op.noise_tip",
            TerraOp::Stamp => "tf.op.stamp_tip",
            TerraOp::Cliff => "tf.op.cliff_tip",
            TerraOp::Plateau => "tf.op.plateau_tip",
            TerraOp::Clone => "tf.op.clone_tip",
            TerraOp::Mask => "tf.op.mask_tip",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ErodeKind {
    #[default]
    Hydraulic,
    Thermal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NoiseStyle {
    #[default]
    Fbm,
    Ridged,
    Billow,
}

impl NoiseStyle {
    pub fn name(self) -> &'static str {
        match self {
            NoiseStyle::Fbm => "fbm",
            NoiseStyle::Ridged => "ridged",
            NoiseStyle::Billow => "billow",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StampBlend {
    #[default]
    Add,
    Max,
    Min,
    Replace,
}

impl StampBlend {
    pub const ALL: [StampBlend; 4] = [StampBlend::Add, StampBlend::Max, StampBlend::Min, StampBlend::Replace];

    pub fn name(self) -> &'static str {
        match self {
            StampBlend::Add => "add",
            StampBlend::Max => "max",
            StampBlend::Min => "min",
            StampBlend::Replace => "replace",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlateauLevel {
    #[default]
    Stroke,
    Fixed,
}

#[derive(Clone, Debug)]
pub struct TerraformSettings {
    pub op: TerraOp,
    pub strength: f64,
    pub erode_kind: ErodeKind,
    pub erode_strength: f64,
    pub erode_rate: f64,
    pub erode_capacity: f64,
    pub erode_deposition: f64,
    pub erode_evaporation: f64,
    pub erode_inertia: f64,
    pub erode_talus: f64,
    pub terrace_step: f64,
    pub terrace_sharpness: f64,
    pub terrace_offset: f64,
    pub noise_style: NoiseStyle,
    pub noise_scale: f64,
    pub noise_octaves: u32,
    pub noise_roughness: f64,
    pub noise_amplitude: f64,
    pub noise_seed: u64,
    pub stamp_shape: StampShape,
    pub stamp_rotation: f64,
    pub stamp_scale: f64,
    pub stamp_height: f64,
    pub stamp_blend: StampBlend,
    pub stamp_image_path: String,
    pub stamp_image: Option<Arc<StampImage>>,
    pub cliff_strength: f64,
    pub plateau_level: PlateauLevel,
    pub plateau_height: f64,
    pub clone_source: Option<DVec3>,
    pub clone_rotation: f64,
    pub clone_height: f64,
    pub mask_show: bool,
}

impl PartialEq for TerraformSettings {
    fn eq(&self, other: &Self) -> bool {
        self.values_text() == other.values_text()
    }
}

impl Default for TerraformSettings {
    fn default() -> Self {
        Self {
            op: TerraOp::Erode,
            strength: 0.5,
            erode_kind: ErodeKind::Hydraulic,
            erode_strength: 0.5,
            erode_rate: 1200.0,
            erode_capacity: 4.0,
            erode_deposition: 0.3,
            erode_evaporation: 0.02,
            erode_inertia: 0.05,
            erode_talus: 35.0,
            terrace_step: 64.0,
            terrace_sharpness: 0.75,
            terrace_offset: 0.0,
            noise_style: NoiseStyle::Fbm,
            noise_scale: 384.0,
            noise_octaves: 5,
            noise_roughness: 0.5,
            noise_amplitude: 48.0,
            noise_seed: 7,
            stamp_shape: StampShape::Mountain,
            stamp_rotation: 0.0,
            stamp_scale: 1.0,
            stamp_height: 384.0,
            stamp_blend: StampBlend::Max,
            stamp_image_path: String::new(),
            stamp_image: None,
            cliff_strength: 0.5,
            plateau_level: PlateauLevel::Stroke,
            plateau_height: 0.0,
            clone_source: None,
            clone_rotation: 0.0,
            clone_height: 0.0,
            mask_show: true,
        }
    }
}

fn fix(v: f64, lo: f64, hi: f64, default: f64) -> f64 {
    if v.is_finite() { v.clamp(lo, hi) } else { default }
}

pub fn wrap_degrees(v: f64) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    let w = (v + 180.0).rem_euclid(360.0) - 180.0;
    if w <= -180.0 { 180.0 } else { w }
}

fn on_off(b: bool) -> String {
    if b { "on" } else { "off" }.to_owned()
}

fn num(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_owned() } else { s.to_owned() }
}

impl TerraformSettings {
    pub fn sanitize(&mut self) {
        let d = TerraformSettings::default();
        self.strength = fix(self.strength, 0.0, 1.0, d.strength);
        self.erode_strength = fix(self.erode_strength, 0.0, 1.0, d.erode_strength);
        self.erode_rate = fix(self.erode_rate, 1.0, 10000.0, d.erode_rate);
        self.erode_capacity = fix(self.erode_capacity, 0.0, 32.0, d.erode_capacity);
        self.erode_deposition = fix(self.erode_deposition, 0.0, 1.0, d.erode_deposition);
        self.erode_evaporation = fix(self.erode_evaporation, 0.0, 0.5, d.erode_evaporation);
        self.erode_inertia = fix(self.erode_inertia, 0.0, 0.95, d.erode_inertia);
        self.erode_talus = fix(self.erode_talus, 1.0, 89.0, d.erode_talus);
        self.terrace_step = fix(self.terrace_step, 1.0, 4096.0, d.terrace_step);
        self.terrace_sharpness = fix(self.terrace_sharpness, 0.0, 1.0, d.terrace_sharpness);
        self.terrace_offset = fix(self.terrace_offset, -4096.0, 4096.0, d.terrace_offset);
        self.noise_scale = fix(self.noise_scale, 1.0, 65536.0, d.noise_scale);
        self.noise_octaves = self.noise_octaves.clamp(1, 8);
        self.noise_roughness = fix(self.noise_roughness, 0.05, 1.0, d.noise_roughness);
        self.noise_amplitude = fix(self.noise_amplitude, 0.0, 4096.0, d.noise_amplitude);
        self.stamp_rotation = wrap_degrees(self.stamp_rotation);
        self.stamp_scale = fix(self.stamp_scale, 0.1, 8.0, d.stamp_scale);
        self.stamp_height = fix(self.stamp_height, -8192.0, 8192.0, d.stamp_height);
        self.cliff_strength = fix(self.cliff_strength, 0.0, 1.0, d.cliff_strength);
        self.plateau_height = fix(self.plateau_height, -16384.0, 16384.0, d.plateau_height);
        self.clone_rotation = wrap_degrees(self.clone_rotation);
        self.clone_height = fix(self.clone_height, -8192.0, 8192.0, d.clone_height);
        if self.clone_source.is_some_and(|p| !p.is_finite()) {
            self.clone_source = None;
        }
    }

    pub fn rotation(&self) -> f64 {
        match self.op {
            TerraOp::Clone => self.clone_rotation,
            _ => self.stamp_rotation,
        }
    }

    pub fn rotate(&mut self, degrees: f64) -> bool {
        match self.op {
            TerraOp::Stamp => self.stamp_rotation = wrap_degrees(self.stamp_rotation + degrees),
            TerraOp::Clone => self.clone_rotation = wrap_degrees(self.clone_rotation + degrees),
            _ => return false,
        }
        true
    }

    fn values_text(&self) -> Vec<(String, String)> {
        self.values().into_iter().map(|o| (o.key.to_owned(), o.value)).collect()
    }

    pub fn values(&self) -> Vec<ToolOption> {
        vec![
            ToolOption::new(
                "terraform",
                self.op.name(),
                "terraform mode brush: erode (hydraulic or thermal erosion painted locally), terrace (steps), noise \
                 (fractal detail), stamp (a heightmap shape under the cursor), cliff (steepens slopes and sharpens \
                 ridges), plateau (flattens tops above a level, keeps the slopes), clone (copies a region, Alt and a \
                 click sets the source) or mask (protects vertices from every brush and command)",
            ),
            ToolOption::new(
                "terraform_strength",
                num(self.strength),
                "speed of terrace, noise, plateau, clone and mask (0 to 1)",
            ),
            ToolOption::new(
                "erosion_brush",
                match self.erode_kind {
                    ErodeKind::Hydraulic => "hydraulic",
                    ErodeKind::Thermal => "thermal",
                },
                "erode brush: hydraulic (rain droplets carve gullies and drop sediment in valleys) or thermal \
                 (slopes steeper than erosion_talus crumble)",
            ),
            ToolOption::new(
                "erosion_strength",
                num(self.erode_strength),
                "erode brush: how much each droplet or pass digs (0 to 1)",
            ),
            ToolOption::new(
                "erosion_rate",
                num(self.erode_rate),
                "erode brush: droplets per second (hydraulic) or passes per second divided by 50 (thermal), 1 to 10000",
            ),
            ToolOption::new(
                "erosion_capacity",
                num(self.erode_capacity),
                "hydraulic: sediment a droplet carries (0 to 32)",
            ),
            ToolOption::new(
                "erosion_deposition",
                num(self.erode_deposition),
                "hydraulic: share of the extra sediment dropped each step (0 to 1)",
            ),
            ToolOption::new(
                "erosion_evaporation",
                num(self.erode_evaporation),
                "hydraulic: share of the water lost each step (0 to 0.5)",
            ),
            ToolOption::new(
                "erosion_inertia",
                num(self.erode_inertia),
                "hydraulic: how much a droplet keeps its direction instead of following the slope (0 to 0.95)",
            ),
            ToolOption::new(
                "erosion_talus",
                num(self.erode_talus),
                "thermal: steepest stable slope in degrees (1 to 89)",
            ),
            ToolOption::new("terrace_step", num(self.terrace_step), "terrace: height of one step in units (1 to 4096)"),
            ToolOption::new(
                "terrace_sharpness",
                num(self.terrace_sharpness),
                "terrace: 0 gives soft waves, 1 flat steps with vertical risers",
            ),
            ToolOption::new(
                "terrace_offset",
                num(self.terrace_offset),
                "terrace: height of the first step (-4096 to 4096)",
            ),
            ToolOption::new(
                "paint_noise",
                self.noise_style.name(),
                "noise brush: fbm (rolling detail), ridged (sharp crests) or billow (puffy bumps)",
            ),
            ToolOption::new(
                "paint_noise_scale",
                num(self.noise_scale),
                "noise brush: size of the biggest features (1 to 65536)",
            ),
            ToolOption::new(
                "paint_noise_octaves",
                self.noise_octaves.to_string(),
                "noise brush: layers of detail (1 to 8)",
            ),
            ToolOption::new(
                "paint_noise_roughness",
                num(self.noise_roughness),
                "noise brush: weight of each finer layer (0.05 to 1)",
            ),
            ToolOption::new(
                "paint_noise_amplitude",
                num(self.noise_amplitude),
                "noise brush: height of the noise in units (0 to 4096); one stroke never adds more than this",
            ),
            ToolOption::new(
                "paint_noise_seed",
                self.noise_seed.to_string(),
                "noise brush: seed; the noise is in world space so it continues across displacements",
            ),
            ToolOption::new(
                "stamp",
                self.stamp_shape.name(),
                "stamp shape: mountain, hill, crater, mesa, volcano, dune, ridge, rock or image (stamp_image)",
            ),
            ToolOption::new(
                "stamp_rotation",
                num(self.stamp_rotation),
                "stamp rotation in degrees (Shift or Alt and the mouse wheel in the 3D view)",
            ),
            ToolOption::new("stamp_scale", num(self.stamp_scale), "stamp size relative to the brush radius (0.1 to 8)"),
            ToolOption::new(
                "stamp_height",
                num(self.stamp_height),
                "stamp height in units (-8192 to 8192, Shift stamps it upside down)",
            ),
            ToolOption::new(
                "stamp_blend",
                self.stamp_blend.name(),
                "stamp blend: add (on top of the terrain), max (only raises), min (only lowers) or replace",
            ),
            ToolOption::new(
                "stamp_image",
                self.stamp_image_path.clone(),
                "grayscale image used by the image stamp (PNG, TGA, TIFF, EXR, RAW...): setting it imports the image; \
                 white is high",
            ),
            ToolOption::new(
                "cliff_strength",
                num(self.cliff_strength),
                "cliff: how fast slopes steepen and ridges sharpen (0 to 1, Shift softens)",
            ),
            ToolOption::new(
                "plateau_level",
                match self.plateau_level {
                    PlateauLevel::Stroke => "stroke",
                    PlateauLevel::Fixed => "fixed",
                },
                "plateau: stroke (the height under the cursor when the stroke starts) or fixed (plateau_height)",
            ),
            ToolOption::new(
                "plateau_height",
                num(self.plateau_height),
                "plateau: level along the paint axis in world units for plateau_level fixed",
            ),
            ToolOption::new(
                "clone_source",
                super::level::point_text(self.clone_source),
                "clone: source point \"x y z\" (world) or none; Alt and a click in the 3D view sets it",
            ),
            ToolOption::new("clone_rotation", num(self.clone_rotation), "clone: rotation of the copy in degrees"),
            ToolOption::new("clone_height", num(self.clone_height), "clone: height added to the copy in units"),
            ToolOption::new("mask_show", on_off(self.mask_show), "on: tint the masked vertices in the 3D view"),
        ]
    }

    pub fn set(&mut self, key: &str, value: &str) -> Option<bool> {
        let v = value.trim().to_ascii_lowercase();
        let number = v.parse::<f64>().ok().filter(|n| n.is_finite());
        let within = |lo: f64, hi: f64| number.filter(|n| (lo..=hi).contains(n));
        let flag = match v.as_str() {
            "on" | "true" | "1" => Some(true),
            "off" | "false" | "0" => Some(false),
            _ => None,
        };
        let done = match key {
            "terraform" => TerraOp::from_name(&v).map(|o| self.op = o).is_some(),
            "terraform_strength" => within(0.0, 1.0).map(|n| self.strength = n).is_some(),
            "erosion_brush" => match v.as_str() {
                "hydraulic" => Some(ErodeKind::Hydraulic),
                "thermal" => Some(ErodeKind::Thermal),
                _ => None,
            }
            .map(|k| self.erode_kind = k)
            .is_some(),
            "erosion_strength" => within(0.0, 1.0).map(|n| self.erode_strength = n).is_some(),
            "erosion_rate" => within(1.0, 10000.0).map(|n| self.erode_rate = n).is_some(),
            "erosion_capacity" => within(0.0, 32.0).map(|n| self.erode_capacity = n).is_some(),
            "erosion_deposition" => within(0.0, 1.0).map(|n| self.erode_deposition = n).is_some(),
            "erosion_evaporation" => within(0.0, 0.5).map(|n| self.erode_evaporation = n).is_some(),
            "erosion_inertia" => within(0.0, 0.95).map(|n| self.erode_inertia = n).is_some(),
            "erosion_talus" => within(1.0, 89.0).map(|n| self.erode_talus = n).is_some(),
            "terrace_step" => within(1.0, 4096.0).map(|n| self.terrace_step = n).is_some(),
            "terrace_sharpness" => within(0.0, 1.0).map(|n| self.terrace_sharpness = n).is_some(),
            "terrace_offset" => within(-4096.0, 4096.0).map(|n| self.terrace_offset = n).is_some(),
            "paint_noise" => match v.as_str() {
                "fbm" | "fractal" => Some(NoiseStyle::Fbm),
                "ridged" => Some(NoiseStyle::Ridged),
                "billow" => Some(NoiseStyle::Billow),
                _ => None,
            }
            .map(|s| self.noise_style = s)
            .is_some(),
            "paint_noise_scale" => within(1.0, 65536.0).map(|n| self.noise_scale = n).is_some(),
            "paint_noise_octaves" => {
                v.parse::<u32>().ok().filter(|n| (1..=8).contains(n)).map(|n| self.noise_octaves = n).is_some()
            }
            "paint_noise_roughness" => within(0.05, 1.0).map(|n| self.noise_roughness = n).is_some(),
            "paint_noise_amplitude" => within(0.0, 4096.0).map(|n| self.noise_amplitude = n).is_some(),
            "paint_noise_seed" => {
                v.parse::<u64>().ok().filter(|n| *n <= u64::from(u32::MAX)).map(|n| self.noise_seed = n).is_some()
            }
            "stamp" => StampShape::from_name(&v).map(|s| self.stamp_shape = s).is_some(),
            "stamp_rotation" => within(-3600.0, 3600.0).map(|n| self.stamp_rotation = wrap_degrees(n)).is_some(),
            "stamp_scale" => within(0.1, 8.0).map(|n| self.stamp_scale = n).is_some(),
            "stamp_height" => within(-8192.0, 8192.0).map(|n| self.stamp_height = n).is_some(),
            "stamp_blend" => {
                StampBlend::ALL.into_iter().find(|b| b.name() == v).map(|b| self.stamp_blend = b).is_some()
            }
            "stamp_image" => {
                let path = value.trim().trim_matches('"');
                if path.is_empty() || path.eq_ignore_ascii_case("none") {
                    self.stamp_image_path.clear();
                    self.stamp_image = None;
                    true
                } else {
                    match StampImage::load(std::path::Path::new(path)) {
                        Ok(img) => {
                            self.stamp_image_path = path.to_owned();
                            self.stamp_image = Some(Arc::new(img));
                            self.stamp_shape = StampShape::Image;
                            true
                        }
                        Err(_) => false,
                    }
                }
            }
            "cliff_strength" => within(0.0, 1.0).map(|n| self.cliff_strength = n).is_some(),
            "plateau_level" => match v.as_str() {
                "stroke" | "cursor" => Some(PlateauLevel::Stroke),
                "fixed" => Some(PlateauLevel::Fixed),
                _ => None,
            }
            .map(|l| self.plateau_level = l)
            .is_some(),
            "plateau_height" => within(-16384.0, 16384.0).map(|n| self.plateau_height = n).is_some(),
            "clone_source" => super::level::parse_point(&v).map(|p| self.clone_source = p).is_some(),
            "clone_rotation" => within(-3600.0, 3600.0).map(|n| self.clone_rotation = wrap_degrees(n)).is_some(),
            "clone_height" => within(-8192.0, 8192.0).map(|n| self.clone_height = n).is_some(),
            "mask_show" => flag.map(|f| self.mask_show = f).is_some(),
            _ => return None,
        };
        Some(done)
    }

    pub fn restore_image(&mut self) {
        if self.stamp_image.is_none() && !self.stamp_image_path.is_empty() {
            self.stamp_image = StampImage::load(std::path::Path::new(&self.stamp_image_path)).ok().map(Arc::new);
        }
    }
}
