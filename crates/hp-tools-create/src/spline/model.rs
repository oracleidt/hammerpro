use glam::{DVec2, DVec3};

pub const VERSION: &str = "sp1";
pub const MAX_POINTS: usize = 512;
pub const MAX_MODELS: usize = 16;
pub const MAX_NAME: usize = 128;
pub const MAX_CUSTOM: usize = 32;
pub const MAX_SIDES: u32 = 32;
pub const MIN_SIDES: u32 = 3;
pub const MAX_SIZE: f64 = 2048.0;
pub const MAX_SPACING: f64 = 8192.0;
pub const MIN_SPACING: f64 = 4.0;
pub const MAX_SEGMENT: f64 = 2048.0;
pub const MIN_SEGMENT: f64 = 4.0;
pub const MAX_ROLL: f64 = 180.0;
pub const MAX_WIDTH: f64 = 16.0;
pub const MIN_WIDTH: f64 = 0.05;
pub const MAX_SLACK: f64 = 4096.0;
pub const MAX_OFFSET: f64 = 4096.0;
pub const MAX_COORD: f64 = hp_geom::MAX_COORD;
pub const DEFAULT_MATERIAL: &str = "dev/dev_measuregeneric01b";
pub const DEFAULT_ROPE_MATERIAL: &str = "cable/cable";
pub const DEFAULT_FENCE_MODEL: &str = "models/props_c17/fence01a.mdl";
pub const DEFAULT_PROP_MODEL: &str = "models/props_c17/lamppost03a_off.mdl";
pub const DEFAULT_PROP_CLASS: &str = "prop_static";

macro_rules! named_enum {
    ($name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub enum $name {
            #[default]
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];
            pub const NAMES: &'static [&'static str] = &[$($text),+];

            pub fn name(self) -> &'static str {
                match self {
                    $($name::$variant => $text),+
                }
            }

            pub fn parse(text: &str) -> Option<Self> {
                let t = text.trim();
                Self::ALL.iter().copied().find(|v| v.name().eq_ignore_ascii_case(t))
            }
        }
    };
}

named_enum!(Interp { CatmullRom => "catmull_rom", Bezier => "bezier" });
named_enum!(Mode { Path => "path", Fence => "fence", Cable => "cable", Sweep => "sweep", Props => "props" });
named_enum!(FenceStyle { Model => "model", Rails => "rails" });
named_enum!(CableStyle { Rope => "rope", Brush => "brush" });
named_enum!(Profile { Pipe => "pipe", Curb => "curb", Trim => "trim", RailTube => "rail_tube", Custom => "custom" });
named_enum!(ModelAxis { Auto => "auto", X => "x", Y => "y" });
named_enum!(Sides { Center => "center", Left => "left", Right => "right", Both => "both" });

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SplinePoint {
    pub pos: DVec3,
    pub roll: f64,
    pub width: f64,
    pub handle_in: Option<DVec3>,
    pub handle_out: Option<DVec3>,
}

impl SplinePoint {
    pub fn new(pos: DVec3) -> Self {
        Self { pos, roll: 0.0, width: 1.0, handle_in: None, handle_out: None }
    }

    pub fn is_valid(&self) -> bool {
        let ok = |v: DVec3| v.is_finite() && v.abs().max_element() <= MAX_COORD;
        ok(self.pos)
            && self.roll.is_finite()
            && self.width.is_finite()
            && self.handle_in.is_none_or(ok)
            && self.handle_out.is_none_or(ok)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SplineMeta {
    pub points: Vec<SplinePoint>,
    pub closed: bool,
    pub interp: Interp,
    pub follow_terrain: bool,
    pub terrain_offset: f64,
    pub mode: Mode,
    pub material: String,
    pub tex_scale: f64,
    pub detail: bool,
    pub seed: u32,
    pub align_slope: bool,
    pub offset_x: f64,
    pub offset_z: f64,
    pub profile: Profile,
    pub sides: u32,
    pub radius: f64,
    pub size_w: f64,
    pub size_h: f64,
    pub segment: f64,
    pub custom: Vec<DVec2>,
    pub fence: FenceStyle,
    pub model: String,
    pub spacing: f64,
    pub fit_spacing: bool,
    pub model_axis: ModelAxis,
    pub post_model: String,
    pub posts_at_points: bool,
    pub post_every_segment: bool,
    pub start_cap: String,
    pub end_cap: String,
    pub prop_class: String,
    pub rail_count: u32,
    pub post_height: f64,
    pub post_size: f64,
    pub rail_size: f64,
    pub post_spacing: f64,
    pub cable: CableStyle,
    pub slack: f64,
    pub rope_width: f64,
    pub rope_material: String,
    pub subdiv: u32,
    pub rope_every: f64,
    pub models: Vec<String>,
    pub sides_mode: Sides,
    pub jitter: f64,
    pub yaw_jitter: f64,
    pub face_path: bool,
    pub scale_min: f64,
    pub scale_max: f64,
    pub drop: bool,
    pub avoid_overlap: bool,
}

impl Default for SplineMeta {
    fn default() -> Self {
        Self {
            points: Vec::new(),
            closed: false,
            interp: Interp::CatmullRom,
            follow_terrain: false,
            terrain_offset: 0.0,
            mode: Mode::Path,
            material: DEFAULT_MATERIAL.to_owned(),
            tex_scale: hp_geom::DEFAULT_TEXTURE_SCALE,
            detail: true,
            seed: 1,
            align_slope: true,
            offset_x: 0.0,
            offset_z: 0.0,
            profile: Profile::Pipe,
            sides: 8,
            radius: 8.0,
            size_w: 16.0,
            size_h: 8.0,
            segment: 32.0,
            custom: Vec::new(),
            fence: FenceStyle::Model,
            model: DEFAULT_FENCE_MODEL.to_owned(),
            spacing: 0.0,
            fit_spacing: true,
            model_axis: ModelAxis::Auto,
            post_model: String::new(),
            posts_at_points: false,
            post_every_segment: false,
            start_cap: String::new(),
            end_cap: String::new(),
            prop_class: DEFAULT_PROP_CLASS.to_owned(),
            rail_count: 2,
            post_height: 48.0,
            post_size: 4.0,
            rail_size: 3.0,
            post_spacing: 96.0,
            cable: CableStyle::Rope,
            slack: 64.0,
            rope_width: 2.0,
            rope_material: DEFAULT_ROPE_MATERIAL.to_owned(),
            subdiv: 2,
            rope_every: 0.0,
            models: vec![DEFAULT_PROP_MODEL.to_owned()],
            sides_mode: Sides::Center,
            jitter: 0.0,
            yaw_jitter: 0.0,
            face_path: true,
            scale_min: 1.0,
            scale_max: 1.0,
            drop: true,
            avoid_overlap: true,
        }
    }
}

fn clean_name(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() && !matches!(c, '|' | ';' | ',' | '"'))
        .take(MAX_NAME)
        .collect::<String>()
        .trim()
        .replace('\\', "/")
}

fn fin(v: f64, lo: f64, hi: f64, default: f64) -> f64 {
    if v.is_finite() { v.clamp(lo, hi) } else { default }
}

pub fn parse_flag(v: &str) -> Option<bool> {
    match v.trim().to_ascii_lowercase().as_str() {
        "1" | "on" | "true" | "yes" => Some(true),
        "0" | "off" | "false" | "no" => Some(false),
        _ => None,
    }
}

fn parse_num(v: &str) -> Option<f64> {
    v.trim().parse::<f64>().ok().filter(|x| x.is_finite())
}

fn fmt(x: f64) -> String {
    let r = (x * 1.0e6).round() / 1.0e6;
    let r = if r == 0.0 { 0.0 } else { r };
    format!("{r}")
}

fn fmt_v(v: DVec3) -> String {
    format!("{},{},{}", fmt(v.x), fmt(v.y), fmt(v.z))
}

fn flag(b: bool) -> &'static str {
    if b { "1" } else { "0" }
}

pub fn parse_list(v: &str) -> Vec<String> {
    v.split([',', ';', '\n']).map(clean_name).filter(|s| !s.is_empty()).take(MAX_MODELS).collect()
}

pub fn parse_custom(v: &str) -> Option<Vec<DVec2>> {
    let mut out = Vec::new();
    for item in v.split([' ', ';']).filter(|s| !s.trim().is_empty()) {
        let (a, b) = item.split_once([':', ','])?;
        let p = DVec2::new(parse_num(a)?, parse_num(b)?);
        if p.abs().max_element() > MAX_SIZE {
            return None;
        }
        out.push(p);
        if out.len() > MAX_CUSTOM {
            return None;
        }
    }
    Some(out)
}

pub fn format_custom(points: &[DVec2]) -> String {
    points.iter().map(|p| format!("{}:{}", fmt(p.x), fmt(p.y))).collect::<Vec<_>>().join(" ")
}

pub struct OptionInfo {
    pub key: &'static str,
    pub takes: &'static str,
}

pub const OPTIONS: &[OptionInfo] = &[
    OptionInfo { key: "mode", takes: "path (the spline alone), fence, cable, sweep or props" },
    OptionInfo { key: "interp", takes: "catmull_rom (through the points) or bezier (with handles)" },
    OptionInfo { key: "closed", takes: "on: a closed loop" },
    OptionInfo { key: "follow_terrain", takes: "on: the path hugs the ground under it between the points" },
    OptionInfo { key: "terrain_offset", takes: "height above the ground when follow_terrain is on" },
    OptionInfo { key: "material", takes: "material of the generated brushes" },
    OptionInfo { key: "tex_scale", takes: "texture scale of the generated brushes (0.01 to 64)" },
    OptionInfo { key: "detail", takes: "on: generated brushes go in one func_detail (default on)" },
    OptionInfo { key: "seed", takes: "random seed of the props mode" },
    OptionInfo { key: "align_slope", takes: "on: models and posts tilt with the slope of the path" },
    OptionInfo { key: "offset_x", takes: "sideways offset from the path (positive: right)" },
    OptionInfo { key: "offset_z", takes: "height offset from the path" },
    OptionInfo { key: "profile", takes: "sweep profile: pipe, curb, trim, rail_tube or custom" },
    OptionInfo { key: "sides", takes: "sides of a round profile (3 to 32)" },
    OptionInfo { key: "radius", takes: "radius of a round profile, a brush cable or a rail tube" },
    OptionInfo { key: "width", takes: "width of the curb, trim or custom profile scale" },
    OptionInfo { key: "height", takes: "height of the curb or trim profile" },
    OptionInfo { key: "segment", takes: "length of one brush along the path (4 to 2048)" },
    OptionInfo { key: "custom", takes: "custom convex profile as x:y pairs separated by spaces" },
    OptionInfo { key: "fence", takes: "fence style: model (a repeated model) or rails (brush posts and rails)" },
    OptionInfo { key: "model", takes: "model repeated along a fence" },
    OptionInfo { key: "spacing", takes: "distance between fence segments or props (0: the model length)" },
    OptionInfo { key: "fit_spacing", takes: "on: the spacing is stretched so the segments end on the path end" },
    OptionInfo { key: "model_axis", takes: "axis along which the fence model is long: auto, x or y" },
    OptionInfo { key: "post_model", takes: "model of the fence posts (empty: no posts)" },
    OptionInfo { key: "posts_at_points", takes: "on: a post on every point of the spline" },
    OptionInfo { key: "post_every_segment", takes: "on: a post between every two fence segments" },
    OptionInfo { key: "start_cap", takes: "model placed at the start of the fence" },
    OptionInfo { key: "end_cap", takes: "model placed at the end of the fence" },
    OptionInfo { key: "prop_class", takes: "class of the placed models: prop_static, prop_dynamic or prop_physics" },
    OptionInfo { key: "rail_count", takes: "rails of a brush fence (1 to 8)" },
    OptionInfo { key: "post_height", takes: "height of the posts of a brush fence" },
    OptionInfo { key: "post_size", takes: "thickness of the posts of a brush fence" },
    OptionInfo { key: "rail_size", takes: "thickness of the rails of a brush fence" },
    OptionInfo { key: "post_spacing", takes: "distance between the posts of a brush fence" },
    OptionInfo { key: "cable", takes: "cable style: rope (move_rope and keyframe_rope) or brush" },
    OptionInfo { key: "slack", takes: "extra rope length between two points (sag)" },
    OptionInfo { key: "rope_width", takes: "width of the rope" },
    OptionInfo { key: "rope_material", takes: "material of the rope" },
    OptionInfo { key: "subdiv", takes: "rope subdivision (0 to 8)" },
    OptionInfo { key: "rope_every", takes: "a rope point every N units along the spline (0: at the points)" },
    OptionInfo { key: "models", takes: "models of the props mode, separated by commas" },
    OptionInfo { key: "side", takes: "where props go: center, left, right or both" },
    OptionInfo { key: "jitter", takes: "random offset of each prop in units" },
    OptionInfo { key: "yaw_jitter", takes: "random rotation of each prop in degrees" },
    OptionInfo { key: "face_path", takes: "on: props turn with the path" },
    OptionInfo { key: "scale_min", takes: "smallest random scale of a prop (modelscale)" },
    OptionInfo { key: "scale_max", takes: "largest random scale of a prop" },
    OptionInfo { key: "drop", takes: "on: props are dropped onto the surface under them" },
    OptionInfo { key: "avoid_overlap", takes: "on: props that would overlap another object are skipped" },
];

impl SplineMeta {
    pub fn clamped(mut self) -> Self {
        self.points.retain(SplinePoint::is_valid);
        self.points.truncate(MAX_POINTS);
        for p in &mut self.points {
            p.roll = fin(p.roll, -MAX_ROLL, MAX_ROLL, 0.0);
            p.width = fin(p.width, MIN_WIDTH, MAX_WIDTH, 1.0);
        }
        self.terrain_offset = fin(self.terrain_offset, -MAX_OFFSET, MAX_OFFSET, 0.0);
        self.material = clean_name(&self.material);
        if self.material.is_empty() {
            self.material = DEFAULT_MATERIAL.to_owned();
        }
        self.tex_scale = if self.tex_scale.is_finite() && self.tex_scale.abs() > 1e-6 {
            self.tex_scale.clamp(0.01, 64.0)
        } else {
            hp_geom::DEFAULT_TEXTURE_SCALE
        };
        self.offset_x = fin(self.offset_x, -MAX_OFFSET, MAX_OFFSET, 0.0);
        self.offset_z = fin(self.offset_z, -MAX_OFFSET, MAX_OFFSET, 0.0);
        self.sides = self.sides.clamp(MIN_SIDES, MAX_SIDES);
        self.radius = fin(self.radius, 0.5, MAX_SIZE, 8.0);
        self.size_w = fin(self.size_w, 0.5, MAX_SIZE, 16.0);
        self.size_h = fin(self.size_h, 0.5, MAX_SIZE, 8.0);
        self.segment = fin(self.segment, MIN_SEGMENT, MAX_SEGMENT, 32.0);
        self.custom.retain(|p| p.is_finite());
        self.custom.truncate(MAX_CUSTOM);
        self.model = clean_name(&self.model);
        self.spacing = if self.spacing.is_finite() && self.spacing > 0.0 {
            self.spacing.clamp(MIN_SPACING, MAX_SPACING)
        } else {
            0.0
        };
        self.post_model = clean_name(&self.post_model);
        self.start_cap = clean_name(&self.start_cap);
        self.end_cap = clean_name(&self.end_cap);
        self.prop_class = clean_name(&self.prop_class);
        if self.prop_class.is_empty() {
            self.prop_class = DEFAULT_PROP_CLASS.to_owned();
        }
        self.rail_count = self.rail_count.clamp(1, 8);
        self.post_height = fin(self.post_height, 1.0, MAX_SIZE, 48.0);
        self.post_size = fin(self.post_size, 0.5, MAX_SIZE, 4.0);
        self.rail_size = fin(self.rail_size, 0.5, MAX_SIZE, 3.0);
        self.post_spacing = fin(self.post_spacing, MIN_SPACING, MAX_SPACING, 96.0);
        self.slack = fin(self.slack, 0.0, MAX_SLACK, 64.0);
        self.rope_width = fin(self.rope_width, 0.1, 64.0, 2.0);
        self.rope_material = clean_name(&self.rope_material);
        if self.rope_material.is_empty() {
            self.rope_material = DEFAULT_ROPE_MATERIAL.to_owned();
        }
        self.subdiv = self.subdiv.min(8);
        self.rope_every = if self.rope_every.is_finite() && self.rope_every > 0.0 {
            self.rope_every.clamp(MIN_SPACING, MAX_SPACING)
        } else {
            0.0
        };
        self.models = self.models.iter().map(|m| clean_name(m)).filter(|m| !m.is_empty()).take(MAX_MODELS).collect();
        self.jitter = fin(self.jitter, 0.0, MAX_OFFSET, 0.0);
        self.yaw_jitter = fin(self.yaw_jitter, 0.0, 180.0, 0.0);
        self.scale_min = fin(self.scale_min, 0.05, 100.0, 1.0);
        self.scale_max = fin(self.scale_max, 0.05, 100.0, 1.0).max(self.scale_min);
        self
    }

    pub fn is_closed(&self) -> bool {
        self.closed && self.points.len() >= 3
    }

    pub fn get_option(&self, key: &str) -> Option<String> {
        Some(match key {
            "mode" => self.mode.name().to_owned(),
            "interp" => self.interp.name().to_owned(),
            "closed" => on_off(self.closed),
            "follow_terrain" => on_off(self.follow_terrain),
            "terrain_offset" => fmt(self.terrain_offset),
            "material" => self.material.clone(),
            "tex_scale" => fmt(self.tex_scale),
            "detail" => on_off(self.detail),
            "seed" => self.seed.to_string(),
            "align_slope" => on_off(self.align_slope),
            "offset_x" => fmt(self.offset_x),
            "offset_z" => fmt(self.offset_z),
            "profile" => self.profile.name().to_owned(),
            "sides" => self.sides.to_string(),
            "radius" => fmt(self.radius),
            "width" => fmt(self.size_w),
            "height" => fmt(self.size_h),
            "segment" => fmt(self.segment),
            "custom" => format_custom(&self.custom),
            "fence" => self.fence.name().to_owned(),
            "model" => self.model.clone(),
            "spacing" => fmt(self.spacing),
            "fit_spacing" => on_off(self.fit_spacing),
            "model_axis" => self.model_axis.name().to_owned(),
            "post_model" => self.post_model.clone(),
            "posts_at_points" => on_off(self.posts_at_points),
            "post_every_segment" => on_off(self.post_every_segment),
            "start_cap" => self.start_cap.clone(),
            "end_cap" => self.end_cap.clone(),
            "prop_class" => self.prop_class.clone(),
            "rail_count" => self.rail_count.to_string(),
            "post_height" => fmt(self.post_height),
            "post_size" => fmt(self.post_size),
            "rail_size" => fmt(self.rail_size),
            "post_spacing" => fmt(self.post_spacing),
            "cable" => self.cable.name().to_owned(),
            "slack" => fmt(self.slack),
            "rope_width" => fmt(self.rope_width),
            "rope_material" => self.rope_material.clone(),
            "subdiv" => self.subdiv.to_string(),
            "rope_every" => fmt(self.rope_every),
            "models" => self.models.join(","),
            "side" => self.sides_mode.name().to_owned(),
            "jitter" => fmt(self.jitter),
            "yaw_jitter" => fmt(self.yaw_jitter),
            "face_path" => on_off(self.face_path),
            "scale_min" => fmt(self.scale_min),
            "scale_max" => fmt(self.scale_max),
            "drop" => on_off(self.drop),
            "avoid_overlap" => on_off(self.avoid_overlap),
            _ => return None,
        })
    }

    pub fn set_option(&mut self, key: &str, value: &str) -> Result<(), String> {
        let v = value.trim();
        let bad = || format!("{key}: {v:?} is not a valid value");
        let num = |lo: f64, hi: f64| parse_num(v).filter(|x| (lo..=hi).contains(x)).ok_or_else(bad);
        let int = |lo: u32, hi: u32| {
            parse_num(v)
                .filter(|x| x.fract() == 0.0 && *x >= f64::from(lo) && *x <= f64::from(hi))
                .map(|x| x as u32)
                .ok_or_else(bad)
        };
        let fl = || parse_flag(v).ok_or_else(bad);
        let name = || {
            let n = clean_name(v);
            if n.chars().count() > MAX_NAME { Err(bad()) } else { Ok(n) }
        };
        match key {
            "mode" => self.mode = Mode::parse(v).ok_or_else(bad)?,
            "interp" => self.interp = Interp::parse(v).ok_or_else(bad)?,
            "closed" => self.closed = fl()?,
            "follow_terrain" => self.follow_terrain = fl()?,
            "terrain_offset" => self.terrain_offset = num(-MAX_OFFSET, MAX_OFFSET)?,
            "material" => {
                let n = name()?;
                if n.is_empty() {
                    return Err(bad());
                }
                self.material = n;
            }
            "tex_scale" => self.tex_scale = num(0.01, 64.0)?,
            "detail" => self.detail = fl()?,
            "seed" => self.seed = int(0, u32::MAX)?,
            "align_slope" => self.align_slope = fl()?,
            "offset_x" => self.offset_x = num(-MAX_OFFSET, MAX_OFFSET)?,
            "offset_z" => self.offset_z = num(-MAX_OFFSET, MAX_OFFSET)?,
            "profile" => self.profile = Profile::parse(v).ok_or_else(bad)?,
            "sides" => self.sides = int(MIN_SIDES, MAX_SIDES)?,
            "radius" => self.radius = num(0.5, MAX_SIZE)?,
            "width" => self.size_w = num(0.5, MAX_SIZE)?,
            "height" => self.size_h = num(0.5, MAX_SIZE)?,
            "segment" => self.segment = num(MIN_SEGMENT, MAX_SEGMENT)?,
            "custom" => self.custom = parse_custom(v).ok_or_else(bad)?,
            "fence" => self.fence = FenceStyle::parse(v).ok_or_else(bad)?,
            "model" => self.model = name()?,
            "spacing" => {
                let x = num(0.0, MAX_SPACING)?;
                self.spacing = if x > 0.0 { x.max(MIN_SPACING) } else { 0.0 };
            }
            "fit_spacing" => self.fit_spacing = fl()?,
            "model_axis" => self.model_axis = ModelAxis::parse(v).ok_or_else(bad)?,
            "post_model" => self.post_model = name()?,
            "posts_at_points" => self.posts_at_points = fl()?,
            "post_every_segment" => self.post_every_segment = fl()?,
            "start_cap" => self.start_cap = name()?,
            "end_cap" => self.end_cap = name()?,
            "prop_class" => {
                let n = name()?;
                if n.is_empty() {
                    return Err(bad());
                }
                self.prop_class = n;
            }
            "rail_count" => self.rail_count = int(1, 8)?,
            "post_height" => self.post_height = num(1.0, MAX_SIZE)?,
            "post_size" => self.post_size = num(0.5, MAX_SIZE)?,
            "rail_size" => self.rail_size = num(0.5, MAX_SIZE)?,
            "post_spacing" => self.post_spacing = num(MIN_SPACING, MAX_SPACING)?,
            "cable" => self.cable = CableStyle::parse(v).ok_or_else(bad)?,
            "slack" => self.slack = num(0.0, MAX_SLACK)?,
            "rope_width" => self.rope_width = num(0.1, 64.0)?,
            "rope_material" => {
                let n = name()?;
                if n.is_empty() {
                    return Err(bad());
                }
                self.rope_material = n;
            }
            "subdiv" => self.subdiv = int(0, 8)?,
            "rope_every" => {
                let x = num(0.0, MAX_SPACING)?;
                self.rope_every = if x > 0.0 { x.max(MIN_SPACING) } else { 0.0 };
            }
            "models" => {
                let list = parse_list(v);
                if list.is_empty() {
                    return Err(bad());
                }
                self.models = list;
            }
            "side" => self.sides_mode = Sides::parse(v).ok_or_else(bad)?,
            "jitter" => self.jitter = num(0.0, MAX_OFFSET)?,
            "yaw_jitter" => self.yaw_jitter = num(0.0, 180.0)?,
            "face_path" => self.face_path = fl()?,
            "scale_min" => {
                self.scale_min = num(0.05, 100.0)?;
                self.scale_max = self.scale_max.max(self.scale_min);
            }
            "scale_max" => {
                self.scale_max = num(0.05, 100.0)?;
                self.scale_min = self.scale_min.min(self.scale_max);
            }
            "drop" => self.drop = fl()?,
            "avoid_overlap" => self.avoid_overlap = fl()?,
            _ => return Err(format!("unknown spline option {key:?}")),
        }
        Ok(())
    }

    pub fn encode(&self) -> String {
        let mut out = String::from(VERSION);
        for info in OPTIONS {
            if let Some(v) = self.get_option(info.key) {
                let v = match info.key {
                    "closed" | "follow_terrain" | "detail" | "align_slope" | "fit_spacing" | "posts_at_points"
                    | "post_every_segment" | "face_path" | "drop" | "avoid_overlap" => flag(v == "on").to_owned(),
                    _ => v,
                };
                out.push('|');
                out.push_str(info.key);
                out.push('=');
                out.push_str(&v);
            }
        }
        out.push_str("|pts=");
        for (i, p) in self.points.iter().enumerate() {
            if i > 0 {
                out.push(';');
            }
            out.push_str(&format!("{},{},{}", fmt_v(p.pos), fmt(p.roll), fmt(p.width)));
            if p.handle_in.is_some() || p.handle_out.is_some() {
                let h = |v: Option<DVec3>| v.map_or_else(|| "a,a,a".to_owned(), fmt_v);
                out.push_str(&format!(",{},{}", h(p.handle_in), h(p.handle_out)));
            }
        }
        out
    }

    pub fn is_spline_data(text: &str) -> bool {
        text.trim_start().starts_with("sp1|")
    }

    pub fn decode(text: &str) -> Option<Self> {
        let mut parts = text.trim().split('|');
        if parts.next()? != VERSION {
            return None;
        }
        let mut m = SplineMeta::default();
        let mut seen_points = false;
        for part in parts {
            let (k, v) = part.split_once('=')?;
            if k == "pts" {
                seen_points = true;
                for item in v.split(';').filter(|s| !s.trim().is_empty()) {
                    m.points.push(decode_point(item)?);
                    if m.points.len() > MAX_POINTS {
                        return None;
                    }
                }
                continue;
            }
            if k == "models" && v.is_empty() {
                m.models.clear();
                continue;
            }
            if k == "custom" && v.is_empty() {
                m.custom.clear();
                continue;
            }
            if OPTIONS.iter().any(|o| o.key == k) {
                m.set_option(k, v).ok()?;
            }
        }
        seen_points.then(|| m.clamped())
    }
}

fn on_off(b: bool) -> String {
    if b { "on" } else { "off" }.to_owned()
}

fn decode_point(item: &str) -> Option<SplinePoint> {
    let f: Vec<&str> = item.split(',').map(str::trim).collect();
    if f.len() != 5 && f.len() != 11 {
        return None;
    }
    let n = |i: usize| parse_num(f[i]);
    let mut p = SplinePoint::new(DVec3::new(n(0)?, n(1)?, n(2)?));
    p.roll = n(3)?;
    p.width = n(4)?;
    if f.len() == 11 {
        let h = |i: usize| -> Option<Option<DVec3>> {
            if f[i] == "a" {
                return Some(None);
            }
            Some(Some(DVec3::new(n(i)?, n(i + 1)?, n(i + 2)?)))
        };
        p.handle_in = h(5)?;
        p.handle_out = h(8)?;
    }
    p.is_valid().then_some(p)
}

pub fn ascii_safe(s: &str) -> bool {
    s.is_ascii() && !s.contains('"')
}
