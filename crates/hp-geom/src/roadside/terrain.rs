use std::collections::HashMap;

use glam::DVec2;

use super::error::RoadsideError;
use super::spine::Spine;

pub const MAX_SHOULDER: f64 = 4096.0;
pub const MAX_MARGIN: f64 = 1024.0;
pub const MAX_SINK: f64 = 256.0;
const MIN_CELL: f64 = 64.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShoulderFalloff {
    #[default]
    Smooth,
    Linear,
    EaseIn,
    EaseOut,
}

impl ShoulderFalloff {
    pub const ALL: [ShoulderFalloff; 4] =
        [ShoulderFalloff::Smooth, ShoulderFalloff::Linear, ShoulderFalloff::EaseIn, ShoulderFalloff::EaseOut];

    pub fn name(self) -> &'static str {
        match self {
            ShoulderFalloff::Smooth => "smooth",
            ShoulderFalloff::Linear => "linear",
            ShoulderFalloff::EaseIn => "ease_in",
            ShoulderFalloff::EaseOut => "ease_out",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.name().eq_ignore_ascii_case(s.trim()))
    }

    pub fn weight(self, t: f64) -> f64 {
        if !t.is_finite() || t >= 1.0 {
            return 0.0;
        }
        let t = t.max(0.0);
        let s = 1.0 - t;
        match self {
            ShoulderFalloff::Smooth => s * s * (3.0 - 2.0 * s),
            ShoulderFalloff::Linear => s,
            ShoulderFalloff::EaseIn => s * s,
            ShoulderFalloff::EaseOut => 1.0 - t * t,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeightMode {
    #[default]
    Both,
    SinkOnly,
    RaiseOnly,
}

impl HeightMode {
    pub const ALL: [HeightMode; 3] = [HeightMode::Both, HeightMode::SinkOnly, HeightMode::RaiseOnly];

    pub fn name(self) -> &'static str {
        match self {
            HeightMode::Both => "both",
            HeightMode::SinkOnly => "sink",
            HeightMode::RaiseOnly => "raise",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name().eq_ignore_ascii_case(s.trim()))
    }

    pub fn allows(self, from: f64, to: f64) -> bool {
        match self {
            HeightMode::Both => true,
            HeightMode::SinkOnly => to < from,
            HeightMode::RaiseOnly => to > from,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlphaMode {
    #[default]
    Off,
    TwoWay,
    FourWay,
}

impl AlphaMode {
    pub const ALL: [AlphaMode; 3] = [AlphaMode::Off, AlphaMode::TwoWay, AlphaMode::FourWay];

    pub fn name(self) -> &'static str {
        match self {
            AlphaMode::Off => "off",
            AlphaMode::TwoWay => "two_way",
            AlphaMode::FourWay => "four_way",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name().eq_ignore_ascii_case(s.trim()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendShape {
    pub shoulder: f64,
    pub margin: f64,
    pub falloff: ShoulderFalloff,
    pub strength: f64,
    pub sink: f64,
    pub mode: HeightMode,
    pub alpha: AlphaMode,
    pub alpha_shoulder: f64,
    pub alpha_strength: f64,
    pub road_channel: usize,
}

impl Default for BlendShape {
    fn default() -> Self {
        Self {
            shoulder: 128.0,
            margin: 0.0,
            falloff: ShoulderFalloff::Smooth,
            strength: 1.0,
            sink: 1.0,
            mode: HeightMode::Both,
            alpha: AlphaMode::Off,
            alpha_shoulder: 64.0,
            alpha_strength: 1.0,
            road_channel: 1,
        }
    }
}

impl BlendShape {
    pub fn validate(&self) -> Result<(), RoadsideError> {
        let chk = |v: f64, lo: f64, hi: f64, what: &'static str| {
            if v.is_finite() && (lo..=hi).contains(&v) { Ok(()) } else { Err(RoadsideError::BadValue(what)) }
        };
        chk(self.shoulder, 0.0, MAX_SHOULDER, "the shoulder width must be 0 to 4096 units")?;
        chk(self.margin, 0.0, MAX_MARGIN, "the flat margin must be 0 to 1024 units")?;
        chk(self.strength, 0.0, 1.0, "the strength must be 0 to 1")?;
        chk(self.sink, -MAX_SINK, MAX_SINK, "the sink under the road must be -256 to 256 units")?;
        chk(self.alpha_shoulder, 0.0, MAX_SHOULDER, "the texture blend width must be 0 to 4096 units")?;
        chk(self.alpha_strength, 0.0, 1.0, "the texture strength must be 0 to 1")?;
        if self.road_channel > 3 {
            return Err(RoadsideError::BadValue("the road texture channel must be 0 to 3"));
        }
        Ok(())
    }

    pub fn reach(&self, half_width: f64) -> f64 {
        half_width
            + self.margin
            + self.shoulder.max(if self.alpha == AlphaMode::Off { 0.0 } else { self.alpha_shoulder })
    }

    pub fn weight(&self, dist: f64, half_width: f64, shoulder: f64) -> f64 {
        let edge = half_width + self.margin;
        if dist <= edge {
            1.0
        } else if shoulder <= 1e-9 {
            0.0
        } else {
            self.falloff.weight((dist - edge) / shoulder)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub height_weight: f64,
    pub alpha_weight: f64,
    pub z: f64,
    pub dist: f64,
    pub spine: usize,
}

struct Grid {
    cell: f64,
    cells: HashMap<(i64, i64), Vec<u32>>,
}

impl Grid {
    fn key(&self, p: DVec2) -> (i64, i64) {
        ((p.x / self.cell).floor() as i64, (p.y / self.cell).floor() as i64)
    }
}

pub struct RoadField<'a> {
    spines: &'a [Spine],
    shape: BlendShape,
    grids: Vec<Grid>,
}

impl<'a> RoadField<'a> {
    pub fn new(spines: &'a [Spine], shape: BlendShape) -> Self {
        let grids = spines
            .iter()
            .map(|s| {
                let reach = shape.reach(s.half_width);
                let cell = reach.max(MIN_CELL);
                let mut g = Grid { cell, cells: HashMap::new() };
                for seg in 0..s.segments() {
                    let (a, b) = (s.stations[seg], s.stations[seg + 1]);
                    let lo = g.key(a.min(b) - DVec2::splat(reach));
                    let hi = g.key(a.max(b) + DVec2::splat(reach));
                    for x in lo.0..=hi.0 {
                        for y in lo.1..=hi.1 {
                            g.cells.entry((x, y)).or_default().push(seg as u32);
                        }
                    }
                }
                g
            })
            .collect();
        Self { spines, shape, grids }
    }

    pub fn bounds(&self) -> Option<(DVec2, DVec2)> {
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for s in self.spines {
            let (a, b) = s.bounds(self.shape.reach(s.half_width));
            lo = lo.min(a);
            hi = hi.max(b);
        }
        lo.is_finite().then_some((lo, hi))
    }

    pub fn sample(&self, p: DVec2) -> Option<Sample> {
        let mut best: Option<Sample> = None;
        for (i, (spine, grid)) in self.spines.iter().zip(&self.grids).enumerate() {
            let Some(list) = grid.cells.get(&grid.key(p)) else { continue };
            let mut near: Option<super::spine::Nearest> = None;
            for seg in list {
                let n = spine.nearest_in(*seg as usize, p);
                if near.is_none_or(|b| n.dist < b.dist) {
                    near = Some(n);
                }
            }
            let Some(n) = near else { continue };
            let hw = spine.half_width;
            let height_weight = self.shape.weight(n.dist, hw, self.shape.shoulder);
            let alpha_weight = if self.shape.alpha == AlphaMode::Off {
                0.0
            } else {
                self.shape.weight(n.dist, hw, self.shape.alpha_shoulder)
            };
            if height_weight <= 0.0 && alpha_weight <= 0.0 {
                continue;
            }
            let frame = spine.frame_at(n.s, true);
            let z = spine.surface_z(&frame, n.lateral) - self.shape.sink;
            let cand = Sample { height_weight, alpha_weight, z, dist: n.dist, spine: i };
            let rank = |s: &Sample| s.height_weight.max(s.alpha_weight);
            if best.is_none_or(|b| rank(&cand) > rank(&b)) {
                best = Some(cand);
            }
        }
        best
    }

    pub fn new_height(&self, current: f64, sample: &Sample) -> f64 {
        let w = sample.height_weight * self.shape.strength;
        if w <= 0.0 || !self.shape.mode.allows(current, sample.z) {
            return current;
        }
        current + (sample.z - current) * w
    }

    pub fn alpha_amount(&self, sample: &Sample) -> f64 {
        sample.alpha_weight * self.shape.alpha_strength
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn road() -> Vec<Spine> {
        let st = vec![DVec2::new(0.0, 0.0), DVec2::new(500.0, 0.0)];
        vec![Spine::new(st, vec![100.0; 2], vec![0.0; 2], 32.0, 0.0).unwrap()]
    }

    #[test]
    fn falloffs_go_from_one_to_zero() {
        for f in ShoulderFalloff::ALL {
            assert!((f.weight(0.0) - 1.0).abs() < 1e-12, "{f:?}");
            assert_eq!(f.weight(1.0), 0.0);
            assert_eq!(f.weight(f64::NAN), 0.0);
            let mut prev = 1.0;
            for i in 1..=10 {
                let w = f.weight(f64::from(i) / 10.0);
                assert!(w <= prev + 1e-12, "{f:?} must not rise");
                prev = w;
            }
        }
        assert!((ShoulderFalloff::Smooth.weight(0.5) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn height_follows_the_road_inside_and_fades_across_the_shoulder() {
        let spines = road();
        let shape = BlendShape { shoulder: 100.0, sink: 0.0, ..Default::default() };
        let field = RoadField::new(&spines, shape);
        let inside = field.sample(DVec2::new(250.0, 10.0)).unwrap();
        assert!((field.new_height(40.0, &inside) - 100.0).abs() < 1e-9);
        let mid = field.sample(DVec2::new(250.0, 82.0)).unwrap();
        let h = field.new_height(40.0, &mid);
        assert!(h > 40.0 && h < 100.0, "{h}");
        assert!(field.sample(DVec2::new(250.0, 200.0)).is_none());
    }

    #[test]
    fn sink_margin_mode_and_strength_apply() {
        let spines = road();
        let shape = BlendShape { shoulder: 100.0, sink: 2.0, margin: 20.0, ..Default::default() };
        let field = RoadField::new(&spines, shape);
        let s = field.sample(DVec2::new(250.0, 50.0)).unwrap();
        assert_eq!(s.height_weight, 1.0);
        assert!((s.z - 98.0).abs() < 1e-9);
        let only_sink = BlendShape { mode: HeightMode::SinkOnly, ..shape };
        let f2 = RoadField::new(&spines, only_sink);
        let s2 = f2.sample(DVec2::new(250.0, 0.0)).unwrap();
        assert_eq!(f2.new_height(40.0, &s2), 40.0);
        assert!(f2.new_height(150.0, &s2) < 150.0);
        let half = BlendShape { strength: 0.5, sink: 0.0, ..shape };
        let f3 = RoadField::new(&spines, half);
        let s3 = f3.sample(DVec2::new(250.0, 0.0)).unwrap();
        assert!((f3.new_height(0.0, &s3) - 50.0).abs() < 1e-9);
    }

    #[test]
    fn banked_road_slopes_the_target() {
        let st = vec![DVec2::new(0.0, 0.0), DVec2::new(500.0, 0.0)];
        let spines = vec![Spine::new(st, vec![0.0; 2], vec![10.0; 2], 32.0, 0.0).unwrap()];
        let field = RoadField::new(&spines, BlendShape { sink: 0.0, ..Default::default() });
        let left = field.sample(DVec2::new(250.0, 30.0)).unwrap();
        let right = field.sample(DVec2::new(250.0, -30.0)).unwrap();
        assert!(left.z > 4.0 && right.z < -4.0);
    }

    #[test]
    fn alpha_amount_uses_its_own_width() {
        let spines = road();
        let shape =
            BlendShape { alpha: AlphaMode::TwoWay, alpha_shoulder: 20.0, shoulder: 100.0, ..Default::default() };
        let field = RoadField::new(&spines, shape);
        let s = field.sample(DVec2::new(250.0, 46.0)).unwrap();
        assert!(s.height_weight > 0.1);
        assert!(s.alpha_weight < 0.5 && s.alpha_weight > 0.0);
        let far = field.sample(DVec2::new(250.0, 80.0)).unwrap();
        assert_eq!(far.alpha_weight, 0.0);
    }

    #[test]
    fn validation_catches_bad_numbers() {
        assert!(BlendShape::default().validate().is_ok());
        assert!(BlendShape { strength: 2.0, ..Default::default() }.validate().is_err());
        assert!(BlendShape { shoulder: f64::NAN, ..Default::default() }.validate().is_err());
        assert!(BlendShape { road_channel: 4, ..Default::default() }.validate().is_err());
    }
}
