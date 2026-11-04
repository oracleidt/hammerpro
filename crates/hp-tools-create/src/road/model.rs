use glam::{DVec2, DVec3};

use crate::curve::section::Section;

pub const VERSION: &str = "r1";
pub const MAX_POINTS: usize = 512;
pub const MAX_LANES: u32 = 8;
pub const MIN_LANE_WIDTH: f64 = 16.0;
pub const MAX_LANE_WIDTH: f64 = 256.0;
pub const MAX_SHOULDER: f64 = 256.0;
pub const MIN_THICKNESS: f64 = 2.0;
pub const MAX_THICKNESS: f64 = 128.0;
pub const MIN_STEP: f64 = 16.0;
pub const MAX_STEP: f64 = 512.0;
pub const MIN_LINE_WIDTH: f64 = 1.0;
pub const MAX_LINE_WIDTH: f64 = 32.0;
pub const MIN_DASH: f64 = 4.0;
pub const MAX_DASH: f64 = 1024.0;
pub const MAX_LIFT: f64 = 256.0;
pub const MAX_FLARE: f64 = 512.0;
pub const MAX_TOTAL_WIDTH: f64 = 2048.0;
pub const MAX_COORD: f64 = hp_geom::MAX_COORD;
pub const MAX_NAME: usize = 128;
pub const DEFAULT_LINE_MATERIAL: &str = "dev/reflectivity_90";
pub const DEFAULT_MATERIAL: &str = "dev/dev_measuregeneric01b";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoadPoint {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl RoadPoint {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn xy(self) -> DVec2 {
        DVec2::new(self.x, self.y)
    }

    pub fn xyz(self) -> DVec3 {
        DVec3::new(self.x, self.y, self.z)
    }

    pub fn is_valid(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite() && self.xyz().abs().max_element() <= MAX_COORD
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineKind {
    None,
    #[default]
    Solid,
    Dashed,
    Double,
}

impl LineKind {
    pub const ALL: [LineKind; 4] = [LineKind::None, LineKind::Solid, LineKind::Dashed, LineKind::Double];

    pub fn name(self) -> &'static str {
        match self {
            LineKind::None => "none",
            LineKind::Solid => "solid",
            LineKind::Dashed => "dashed",
            LineKind::Double => "double",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name().eq_ignore_ascii_case(s.trim()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JunctionEnd {
    Start,
    End,
}

impl JunctionEnd {
    pub fn name(self) -> &'static str {
        match self {
            JunctionEnd::Start => "start",
            JunctionEnd::End => "end",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "start" => Some(JunctionEnd::Start),
            "end" => Some(JunctionEnd::End),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Attach {
    #[default]
    Outer,
    Lane,
}

impl Attach {
    pub fn name(self) -> &'static str {
        match self {
            Attach::Outer => "outer",
            Attach::Lane => "lane",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "outer" => Some(Attach::Outer),
            "lane" => Some(Attach::Lane),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Junction {
    pub end: JunctionEnd,
    pub target: u32,
    pub flare: f64,
    pub attach: Attach,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadMeta {
    pub uid: u32,
    pub points: Vec<RoadPoint>,
    pub lanes: u32,
    pub lane_width: f64,
    pub shoulder: f64,
    pub thickness: f64,
    pub step: f64,
    pub tex_scale: f64,
    pub material: String,
    pub shoulder_material: String,
    pub line_material: String,
    pub center_material: String,
    pub center: LineKind,
    pub edge: LineKind,
    pub divider: LineKind,
    pub line_width: f64,
    pub dash: f64,
    pub gap: f64,
    pub bank: Section,
    pub follow_terrain: bool,
    pub lift: f64,
    pub start_dir: Option<DVec2>,
    pub end_dir: Option<DVec2>,
    pub start_slope: Option<f64>,
    pub end_slope: Option<f64>,
    pub u0: f64,
    pub junction: Option<Junction>,
}

impl Default for RoadMeta {
    fn default() -> Self {
        Self {
            uid: 1,
            points: Vec::new(),
            lanes: 2,
            lane_width: 128.0,
            shoulder: 16.0,
            thickness: 8.0,
            step: 64.0,
            tex_scale: hp_geom::DEFAULT_TEXTURE_SCALE,
            material: DEFAULT_MATERIAL.to_owned(),
            shoulder_material: String::new(),
            line_material: DEFAULT_LINE_MATERIAL.to_owned(),
            center_material: String::new(),
            center: LineKind::Dashed,
            edge: LineKind::Solid,
            divider: LineKind::Dashed,
            line_width: 6.0,
            dash: 96.0,
            gap: 96.0,
            bank: Section::default(),
            follow_terrain: false,
            lift: 1.0,
            start_dir: None,
            end_dir: None,
            start_slope: None,
            end_slope: None,
            u0: 0.0,
            junction: None,
        }
    }
}

fn fin(v: f64, lo: f64, hi: f64) -> f64 {
    if v.is_finite() { v.clamp(lo, hi) } else { lo }
}

fn clean_name(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect::<String>().trim().to_owned()
}

pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '/' | '.' | '-' | '+') {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

pub fn unescape(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

impl RoadMeta {
    pub fn lane_area(&self) -> f64 {
        f64::from(self.lanes) * self.lane_width
    }

    pub fn total_width(&self) -> f64 {
        self.lane_area() + self.shoulder * 2.0
    }

    pub fn clamped(mut self) -> Self {
        self.lanes = self.lanes.clamp(1, MAX_LANES);
        self.lane_width = fin(self.lane_width, MIN_LANE_WIDTH, MAX_LANE_WIDTH);
        self.shoulder = fin(self.shoulder, 0.0, MAX_SHOULDER);
        self.thickness = fin(self.thickness, MIN_THICKNESS, MAX_THICKNESS);
        self.step = fin(self.step, MIN_STEP, MAX_STEP);
        self.tex_scale = if self.tex_scale.is_finite() && self.tex_scale.abs() > 1e-6 {
            self.tex_scale.clamp(0.01, 64.0)
        } else {
            hp_geom::DEFAULT_TEXTURE_SCALE
        };
        self.line_width = fin(self.line_width, MIN_LINE_WIDTH, MAX_LINE_WIDTH);
        self.dash = fin(self.dash, MIN_DASH, MAX_DASH);
        self.gap = fin(self.gap, MIN_DASH, MAX_DASH);
        self.lift = fin(self.lift, -MAX_LIFT, MAX_LIFT);
        self.u0 = if self.u0.is_finite() { self.u0 } else { 0.0 };
        self.bank = Section { profile: crate::curve::Profile::Flat, ..self.bank }.clamped();
        self.material = clean_name(&self.material);
        self.shoulder_material = clean_name(&self.shoulder_material);
        self.line_material = clean_name(&self.line_material);
        self.center_material = clean_name(&self.center_material);
        if self.material.is_empty() {
            self.material = DEFAULT_MATERIAL.to_owned();
        }
        if self.line_material.is_empty() {
            self.line_material = DEFAULT_LINE_MATERIAL.to_owned();
        }
        self.start_dir = self.start_dir.and_then(|d| d.try_normalize());
        self.end_dir = self.end_dir.and_then(|d| d.try_normalize());
        self.start_slope = self.start_slope.filter(|s| s.is_finite());
        self.end_slope = self.end_slope.filter(|s| s.is_finite());
        if let Some(j) = &mut self.junction {
            j.flare = fin(j.flare, 0.0, MAX_FLARE);
        }
        self
    }

    pub fn shoulder_material_name(&self) -> &str {
        if self.shoulder_material.is_empty() { &self.material } else { &self.shoulder_material }
    }

    pub fn center_material_name(&self) -> &str {
        if self.center_material.is_empty() { &self.line_material } else { &self.center_material }
    }

    pub fn encode(&self) -> String {
        let b = &self.bank;
        let opt = |v: Option<f64>| v.map_or(String::new(), |x| x.to_string());
        let dir = |v: Option<DVec2>| v.map_or(String::new(), |d| format!("{},{}", d.x, d.y));
        let junction = self
            .junction
            .map_or(String::new(), |j| format!("{},{},{},{}", j.end.name(), j.target, j.flare, j.attach.name()));
        let mut out = format!(
            "{VERSION};uid={};lanes={};lw={};sh={};th={};step={};tex={};mat={};smat={};lmat={};cmat={};cl={};el={};dl={};lwid={};dash={};gap={};r0={};r1={};ab={};al={};ft={};lift={};sd={};ed={};ss={};es={};u0={};junc={};pts=",
            self.uid,
            self.lanes,
            self.lane_width,
            self.shoulder,
            self.thickness,
            self.step,
            self.tex_scale,
            escape(&self.material),
            escape(&self.shoulder_material),
            escape(&self.line_material),
            escape(&self.center_material),
            self.center.name(),
            self.edge.name(),
            self.divider.name(),
            self.line_width,
            self.dash,
            self.gap,
            b.roll_start,
            b.roll_end,
            b.auto_gain,
            b.auto_limit,
            u8::from(self.follow_terrain),
            self.lift,
            dir(self.start_dir),
            dir(self.end_dir),
            opt(self.start_slope),
            opt(self.end_slope),
            self.u0,
            junction,
        );
        for (i, p) in self.points.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&format!("{},{},{}", p.x, p.y, p.z));
        }
        out
    }

    pub fn is_road_data(text: &str) -> bool {
        text.trim_start().starts_with("r1;")
    }

    pub fn decode(text: &str) -> Option<Self> {
        let mut parts = text.trim().split(';');
        if parts.next()? != VERSION {
            return None;
        }
        let mut m = RoadMeta::default();
        let mut seen_points = false;
        for part in parts {
            let (k, v) = part.split_once('=')?;
            let num = || v.parse::<f64>().ok().filter(|x| x.is_finite());
            let opt_num = || if v.is_empty() { Some(None) } else { num().map(Some) };
            let opt_dir = || -> Option<Option<DVec2>> {
                if v.is_empty() {
                    return Some(None);
                }
                let (a, b) = v.split_once(',')?;
                let d = DVec2::new(a.parse().ok()?, b.parse().ok()?);
                d.is_finite().then_some(Some(d))
            };
            match k {
                "uid" => m.uid = v.parse().ok()?,
                "lanes" => m.lanes = v.parse().ok()?,
                "lw" => m.lane_width = num()?,
                "sh" => m.shoulder = num()?,
                "th" => m.thickness = num()?,
                "step" => m.step = num()?,
                "tex" => m.tex_scale = num()?,
                "mat" => m.material = unescape(v)?,
                "smat" => m.shoulder_material = unescape(v)?,
                "lmat" => m.line_material = unescape(v)?,
                "cmat" => m.center_material = unescape(v)?,
                "cl" => m.center = LineKind::parse(v)?,
                "el" => m.edge = LineKind::parse(v)?,
                "dl" => m.divider = LineKind::parse(v)?,
                "lwid" => m.line_width = num()?,
                "dash" => m.dash = num()?,
                "gap" => m.gap = num()?,
                "r0" => m.bank.roll_start = num()?,
                "r1" => m.bank.roll_end = num()?,
                "ab" => m.bank.auto_gain = num()?,
                "al" => m.bank.auto_limit = num()?,
                "ft" => m.follow_terrain = v == "1",
                "lift" => m.lift = num()?,
                "sd" => m.start_dir = opt_dir()?,
                "ed" => m.end_dir = opt_dir()?,
                "ss" => m.start_slope = opt_num()?,
                "es" => m.end_slope = opt_num()?,
                "u0" => m.u0 = num()?,
                "junc" => {
                    m.junction = if v.is_empty() {
                        None
                    } else {
                        let mut it = v.split(',');
                        let end = JunctionEnd::parse(it.next()?)?;
                        let target = it.next()?.parse().ok()?;
                        let flare = it.next()?.parse().ok()?;
                        let attach = it.next().map_or(Some(Attach::Outer), Attach::parse)?;
                        Some(Junction { end, target, flare, attach })
                    }
                }
                "pts" => {
                    seen_points = true;
                    for item in v.split_whitespace() {
                        let mut it = item.split(',');
                        let p = RoadPoint::new(
                            it.next()?.parse().ok()?,
                            it.next()?.parse().ok()?,
                            it.next()?.parse().ok()?,
                        );
                        if !p.is_valid() {
                            return None;
                        }
                        m.points.push(p);
                        if m.points.len() > MAX_POINTS {
                            return None;
                        }
                    }
                }
                _ => {}
            }
        }
        (seen_points && m.points.len() >= 2).then(|| m.clamped())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn sample() -> RoadMeta {
        RoadMeta {
            uid: 7,
            points: vec![RoadPoint::new(0.0, 0.5, 4.0), RoadPoint::new(300.0, -64.25, 12.5)],
            lanes: 4,
            material: "road/asphalt 01".to_owned(),
            center: LineKind::Double,
            edge: LineKind::None,
            follow_terrain: true,
            start_dir: Some(DVec2::X),
            end_slope: Some(0.25),
            u0: 96.0,
            junction: Some(Junction { end: JunctionEnd::End, target: 3, flare: 40.0, attach: Attach::Lane }),
            bank: Section { roll_start: -5.5, roll_end: 10.0, auto_gain: 3.0, auto_limit: 9.0, ..Section::default() },
            ..RoadMeta::default()
        }
    }

    #[test]
    fn round_trip() {
        let m = sample().clamped();
        let text = m.encode();
        assert!(text.is_ascii() && !text.contains('"'));
        assert!(RoadMeta::is_road_data(&text));
        assert_eq!(RoadMeta::decode(&text), Some(m));
    }

    #[test]
    fn bad_text_is_refused() {
        assert_eq!(RoadMeta::decode(""), None);
        assert_eq!(RoadMeta::decode("v1;view=top;pts=0,0 1,1"), None);
        assert_eq!(RoadMeta::decode("r1;pts=0,0,0"), None);
        assert_eq!(RoadMeta::decode("r1;pts=0,0,0 nan,1,2"), None);
        assert_eq!(RoadMeta::decode("r1;cl=wavy;pts=0,0,0 1,1,1"), None);
    }

    #[test]
    fn values_are_clamped() {
        let m =
            RoadMeta { lanes: 99, lane_width: f64::NAN, shoulder: -3.0, step: 1.0, ..RoadMeta::default() }.clamped();
        assert_eq!(m.lanes, MAX_LANES);
        assert_eq!(m.lane_width, MIN_LANE_WIDTH);
        assert_eq!(m.shoulder, 0.0);
        assert_eq!(m.step, MIN_STEP);
        assert_eq!(unescape(&escape("a b;c=d")), Some("a b;c=d".to_owned()));
    }
}
