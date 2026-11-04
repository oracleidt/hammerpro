use glam::DVec2;
use hp_app::ViewKind;

use super::geom::{MAX_CURVE_POINTS, clamp_subdivisions, clamp_thickness};
use super::section::{Profile, Section};

const VERSION: &str = "v1";

#[derive(Clone, Debug, PartialEq)]
pub struct CurveMeta {
    pub kind: ViewKind,
    pub points: Vec<DVec2>,
    pub depth: (f64, f64),
    pub segments: u32,
    pub thickness: f64,
    pub tex_scale: f64,
    pub section: Section,
}

fn view_name(kind: ViewKind) -> Option<&'static str> {
    match kind {
        ViewKind::Top => Some("top"),
        ViewKind::Front => Some("front"),
        ViewKind::Side => Some("side"),
        _ => None,
    }
}

fn view_from(name: &str) -> Option<ViewKind> {
    match name {
        "top" => Some(ViewKind::Top),
        "front" => Some(ViewKind::Front),
        "side" => Some(ViewKind::Side),
        _ => None,
    }
}

impl CurveMeta {
    pub fn encode(&self) -> Option<String> {
        let s = &self.section;
        let mut out = format!(
            "{VERSION};view={};depth={},{};sub={};th={};tex={};prof={};cw={};ch={};dd={};r0={};r1={};ab={};al={};pts=",
            view_name(self.kind)?,
            self.depth.0,
            self.depth.1,
            self.segments,
            self.thickness,
            self.tex_scale,
            s.profile.name(),
            s.curb_width,
            s.curb_height,
            s.ditch_depth,
            s.roll_start,
            s.roll_end,
            s.auto_gain,
            s.auto_limit,
        );
        for (i, p) in self.points.iter().enumerate() {
            if i > 0 {
                out.push(' ');
            }
            out.push_str(&format!("{},{}", p.x, p.y));
        }
        Some(out)
    }

    pub fn decode(text: &str) -> Option<Self> {
        let mut parts = text.trim().split(';');
        if parts.next()? != VERSION {
            return None;
        }
        let mut meta = CurveMeta {
            kind: ViewKind::Top,
            points: Vec::new(),
            depth: (0.0, 64.0),
            segments: 16,
            thickness: 16.0,
            tex_scale: hp_geom::DEFAULT_TEXTURE_SCALE,
            section: Section::default(),
        };
        let mut seen_points = false;
        for part in parts {
            let (k, v) = part.split_once('=')?;
            let num = || v.parse::<f64>().ok().filter(|x| x.is_finite());
            match k {
                "view" => meta.kind = view_from(v)?,
                "depth" => {
                    let (a, b) = v.split_once(',')?;
                    meta.depth = (
                        a.parse().ok().filter(|x: &f64| x.is_finite())?,
                        b.parse().ok().filter(|x: &f64| x.is_finite())?,
                    );
                }
                "sub" => meta.segments = clamp_subdivisions(v.parse::<i64>().ok()?),
                "th" => meta.thickness = clamp_thickness(num()?),
                "tex" => meta.tex_scale = num()?,
                "prof" => meta.section.profile = Profile::parse(v)?,
                "cw" => meta.section.curb_width = num()?,
                "ch" => meta.section.curb_height = num()?,
                "dd" => meta.section.ditch_depth = num()?,
                "r0" => meta.section.roll_start = num()?,
                "r1" => meta.section.roll_end = num()?,
                "ab" => meta.section.auto_gain = num()?,
                "al" => meta.section.auto_limit = num()?,
                "pts" => {
                    seen_points = true;
                    for item in v.split_whitespace() {
                        let (x, y) = item.split_once(',')?;
                        let p = DVec2::new(x.parse().ok()?, y.parse().ok()?);
                        if !p.is_finite() {
                            return None;
                        }
                        meta.points.push(p);
                        if meta.points.len() > MAX_CURVE_POINTS {
                            return None;
                        }
                    }
                }
                _ => {}
            }
        }
        meta.section = meta.section.clamped();
        (seen_points && meta.points.len() >= 2).then_some(meta)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn sample() -> CurveMeta {
        CurveMeta {
            kind: ViewKind::Front,
            points: vec![DVec2::new(0.0, 0.5), DVec2::new(128.0, -64.25), DVec2::new(300.0, 12.0)],
            depth: (-16.0, 48.0),
            segments: 24,
            thickness: 96.5,
            tex_scale: 0.25,
            section: Section {
                profile: Profile::Curb,
                curb_width: 12.0,
                curb_height: 6.0,
                ditch_depth: 4.0,
                roll_start: -5.5,
                roll_end: 10.0,
                auto_gain: 3.0,
                auto_limit: 9.0,
            },
        }
    }

    #[test]
    fn round_trip() {
        let m = sample();
        let text = m.encode().unwrap();
        assert!(text.is_ascii() && !text.contains('"'));
        assert_eq!(CurveMeta::decode(&text), Some(m));
    }

    #[test]
    fn bad_text_is_refused() {
        assert_eq!(CurveMeta::decode(""), None);
        assert_eq!(CurveMeta::decode("v2;pts=0,0 1,1"), None);
        assert_eq!(CurveMeta::decode("v1;view=top;pts=0,0"), None);
        assert_eq!(CurveMeta::decode("v1;view=top;pts=0,0 nan,1"), None);
        assert_eq!(CurveMeta::decode("v1;view=3d;pts=0,0 1,1"), None);
    }
}
