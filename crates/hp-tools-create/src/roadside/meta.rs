use hp_geom::roadside::{CurbProfile, GutterProfile, GutterShape, Sides, SidewalkProfile};

use super::source::Source;

pub const VERSION: &str = "rs1";
pub const MAX_NAME: usize = 128;
pub const MAX_SOURCES: usize = 16;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Sidewalk,
    Curb,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Sidewalk => "sidewalk",
            Kind::Curb => "curb",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "sidewalk" => Some(Kind::Sidewalk),
            "curb" => Some(Kind::Curb),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Materials {
    pub top: String,
    pub face: String,
    pub curb: String,
    pub gutter: String,
}

impl Materials {
    pub fn or_top(&self, own: &str) -> String {
        if own.is_empty() { self.top.clone() } else { own.to_owned() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RoadsideMeta {
    pub kind: Kind,
    pub sides: Sides,
    pub gap: f64,
    pub radius: f64,
    pub sidewalk: SidewalkProfile,
    pub curb: CurbProfile,
    pub use_curb: bool,
    pub gutter: GutterProfile,
    pub use_gutter: bool,
    pub tex_scale: f64,
    pub materials: Materials,
    pub sources: Vec<Source>,
}

impl Default for RoadsideMeta {
    fn default() -> Self {
        Self {
            kind: Kind::Sidewalk,
            sides: Sides::Both,
            gap: 0.0,
            radius: 32.0,
            sidewalk: SidewalkProfile::default(),
            curb: CurbProfile::default(),
            use_curb: true,
            gutter: GutterProfile::default(),
            use_gutter: false,
            tex_scale: hp_geom::DEFAULT_TEXTURE_SCALE,
            materials: Materials::default(),
            sources: Vec::new(),
        }
    }
}

fn clean(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control() && !matches!(c, '|' | ';'))
        .take(MAX_NAME)
        .collect::<String>()
        .trim()
        .to_owned()
}

impl RoadsideMeta {
    pub fn encode(&self) -> String {
        let (s, c, g) = (&self.sidewalk, &self.curb, &self.gutter);
        let mut out = format!(
            "{VERSION}|kind={}|sides={}|gap={}|rad={}|sw={}|sh={}|sd={}|sb={}|ss={}|uc={}|cw={}|ch={}|cd={}|cb={}|cr={}|ce={}|ug={}|gw={}|gd={}|gs={}|gf={}|gt={}|gtw={}|gtd={}|gte={}|tex={}|m1={}|m2={}|m3={}|m4={}",
            self.kind.name(),
            self.sides.name(),
            self.gap,
            self.radius,
            s.width,
            s.height,
            s.depth,
            s.bevel,
            s.slope,
            u8::from(self.use_curb),
            c.width,
            c.height,
            c.depth,
            c.bevel,
            u8::from(c.rounded),
            u8::from(c.both_edges),
            u8::from(self.use_gutter),
            g.width,
            g.depth,
            g.shape.name(),
            g.floor_width,
            u8::from(g.slot),
            g.slot_width,
            g.slot_depth,
            g.slot_every,
            self.tex_scale,
            clean(&self.materials.top),
            clean(&self.materials.face),
            clean(&self.materials.curb),
            clean(&self.materials.gutter),
        );
        for src in &self.sources {
            out.push_str("|src=");
            out.push_str(&src.encode());
        }
        out
    }

    pub fn decode(text: &str) -> Option<Self> {
        let mut parts = text.trim().split('|');
        if parts.next()? != VERSION {
            return None;
        }
        let mut m = RoadsideMeta::default();
        for part in parts {
            let (k, v) = part.split_once('=')?;
            let num = || v.parse::<f64>().ok().filter(|x| x.is_finite());
            let flag = || v.parse::<u8>().ok().map(|x| x != 0);
            match k {
                "kind" => m.kind = Kind::parse(v)?,
                "sides" => m.sides = Sides::parse(v)?,
                "gap" => m.gap = num()?,
                "rad" => m.radius = num()?,
                "sw" => m.sidewalk.width = num()?,
                "sh" => m.sidewalk.height = num()?,
                "sd" => m.sidewalk.depth = num()?,
                "sb" => m.sidewalk.bevel = num()?,
                "ss" => m.sidewalk.slope = num()?,
                "uc" => m.use_curb = flag()?,
                "cw" => m.curb.width = num()?,
                "ch" => m.curb.height = num()?,
                "cd" => m.curb.depth = num()?,
                "cb" => m.curb.bevel = num()?,
                "cr" => m.curb.rounded = flag()?,
                "ce" => m.curb.both_edges = flag()?,
                "ug" => m.use_gutter = flag()?,
                "gw" => m.gutter.width = num()?,
                "gd" => m.gutter.depth = num()?,
                "gs" => m.gutter.shape = GutterShape::parse(v)?,
                "gf" => m.gutter.floor_width = num()?,
                "gt" => m.gutter.slot = flag()?,
                "gtw" => m.gutter.slot_width = num()?,
                "gtd" => m.gutter.slot_depth = num()?,
                "gte" => m.gutter.slot_every = v.parse().ok()?,
                "tex" => m.tex_scale = num()?,
                "m1" => m.materials.top = v.to_owned(),
                "m2" => m.materials.face = v.to_owned(),
                "m3" => m.materials.curb = v.to_owned(),
                "m4" => m.materials.gutter = v.to_owned(),
                "src" => {
                    m.sources.push(Source::decode(v)?);
                    if m.sources.len() > MAX_SOURCES {
                        return None;
                    }
                }
                _ => {}
            }
        }
        (!m.sources.is_empty()).then_some(m)
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec2;
    use hp_app::ViewKind;

    use super::*;
    use crate::curve::{CurveMeta, Section};

    fn sample() -> RoadsideMeta {
        let curve = CurveMeta {
            kind: ViewKind::Top,
            points: vec![DVec2::new(0.0, 0.0), DVec2::new(256.0, 64.5)],
            depth: (0.0, 8.0),
            segments: 12,
            thickness: 128.0,
            tex_scale: 0.25,
            section: Section::default(),
        };
        RoadsideMeta {
            kind: Kind::Curb,
            sides: Sides::Left,
            gap: 4.0,
            radius: 40.5,
            use_gutter: true,
            curb: CurbProfile { bevel: 1.5, rounded: true, both_edges: true, ..Default::default() },
            gutter: GutterProfile { shape: GutterShape::U, slot: true, slot_every: 3, ..Default::default() },
            materials: Materials {
                top: "dev/a".into(),
                face: "dev/b".into(),
                curb: String::new(),
                gutter: "dev/d".into(),
            },
            sources: vec![Source::Curve(curve), Source::Road(7)],
            ..Default::default()
        }
    }

    #[test]
    fn round_trip() {
        let m = sample();
        let text = m.encode();
        assert!(text.is_ascii() && !text.contains('"'));
        assert_eq!(RoadsideMeta::decode(&text), Some(m));
    }

    #[test]
    fn bad_text_is_refused() {
        assert_eq!(RoadsideMeta::decode(""), None);
        assert_eq!(RoadsideMeta::decode("rs2|kind=curb|src=r:1"), None);
        assert_eq!(RoadsideMeta::decode("rs1|kind=curb"), None);
        assert_eq!(RoadsideMeta::decode("rs1|kind=wall|src=r:1"), None);
        assert_eq!(RoadsideMeta::decode("rs1|gap=nan|src=r:1"), None);
        assert!(RoadsideMeta::decode("rs1|kind=curb|src=r:1").is_some());
    }

    #[test]
    fn separators_are_removed_from_material_names() {
        let mut m = sample();
        m.materials.top = "a|b;c".into();
        let back = RoadsideMeta::decode(&m.encode()).unwrap();
        assert_eq!(back.materials.top, "abc");
    }
}
