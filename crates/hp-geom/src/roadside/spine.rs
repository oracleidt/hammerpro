use glam::DVec2;

pub const MIN_STRIP: f64 = 1.0;
const TOL: f64 = 1e-9;

#[derive(Clone, Debug, PartialEq)]
pub struct Spine {
    pub stations: Vec<DVec2>,
    pub top: Vec<f64>,
    pub roll: Vec<f64>,
    pub half_width: f64,
    pub ditch: f64,
    cumulative: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub point: DVec2,
    pub dir: DVec2,
    pub left: DVec2,
    pub top: f64,
    pub roll: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nearest {
    pub s: f64,
    pub point: DVec2,
    pub lateral: f64,
    pub dist: f64,
    pub segment: usize,
}

pub fn perp_left(d: DVec2) -> DVec2 {
    DVec2::new(-d.y, d.x)
}

impl Spine {
    pub fn new(stations: Vec<DVec2>, top: Vec<f64>, roll: Vec<f64>, half_width: f64, ditch: f64) -> Option<Self> {
        let n = stations.len();
        if n < 2 || top.len() != n || roll.len() != n {
            return None;
        }
        let ok = stations.iter().all(|p| p.is_finite())
            && top.iter().all(|z| z.is_finite())
            && roll.iter().all(|r| r.is_finite())
            && half_width.is_finite()
            && half_width > 0.0
            && ditch.is_finite();
        if !ok {
            return None;
        }
        let mut cumulative = Vec::with_capacity(n);
        let mut total = 0.0;
        cumulative.push(0.0);
        for w in stations.windows(2) {
            let d = w[0].distance(w[1]);
            if d < TOL {
                return None;
            }
            total += d;
            cumulative.push(total);
        }
        Some(Self { stations, top, roll, half_width, ditch: ditch.max(0.0), cumulative })
    }

    pub fn length(&self) -> f64 {
        self.cumulative.last().copied().unwrap_or(0.0)
    }

    pub fn cumulative(&self) -> &[f64] {
        &self.cumulative
    }

    pub fn segments(&self) -> usize {
        self.stations.len() - 1
    }

    pub fn start(&self) -> DVec2 {
        self.stations[0]
    }

    pub fn end(&self) -> DVec2 {
        self.stations[self.stations.len() - 1]
    }

    pub fn chord_dir(&self, seg: usize) -> DVec2 {
        let seg = seg.min(self.segments() - 1);
        (self.stations[seg + 1] - self.stations[seg]).normalize()
    }

    pub fn segment_at(&self, s: f64, after: bool) -> (usize, f64) {
        let s = s.clamp(0.0, self.length());
        let last = self.segments() - 1;
        let mut seg = 0;
        while seg < last && self.cumulative[seg + 1] < s - TOL {
            seg += 1;
        }
        if after {
            while seg < last && self.cumulative[seg + 1] <= s + TOL {
                seg += 1;
            }
        }
        let len = self.cumulative[seg + 1] - self.cumulative[seg];
        let t = ((s - self.cumulative[seg]) / len).clamp(0.0, 1.0);
        (seg, t)
    }

    pub fn frame_at(&self, s: f64, after: bool) -> Frame {
        let (seg, t) = self.segment_at(s, after);
        let dir = self.chord_dir(seg);
        Frame {
            point: self.stations[seg].lerp(self.stations[seg + 1], t),
            dir,
            left: perp_left(dir),
            top: self.top[seg] + (self.top[seg + 1] - self.top[seg]) * t,
            roll: self.roll[seg] + (self.roll[seg + 1] - self.roll[seg]) * t,
        }
    }

    pub fn edge_shift(&self, roll: f64, side: f64) -> f64 {
        side * self.half_width * roll.to_radians().tan()
    }

    pub fn edge_z(&self, frame: &Frame, side: f64) -> f64 {
        frame.top + self.edge_shift(frame.roll, side)
    }

    pub fn surface_z(&self, frame: &Frame, lateral: f64) -> f64 {
        let h = self.half_width;
        let u = lateral.clamp(-h, h);
        let bank = u * frame.roll.to_radians().tan();
        let dip = if self.ditch > 0.0 { -self.ditch * (1.0 - u.abs() / h) } else { 0.0 };
        frame.top + bank + dip
    }

    pub fn sub(&self, s0: f64, s1: f64) -> Option<Spine> {
        let (s0, s1) = (s0.max(0.0), s1.min(self.length()));
        if s1 - s0 < MIN_STRIP {
            return None;
        }
        let mut stations = Vec::new();
        let mut top = Vec::new();
        let mut roll = Vec::new();
        let a = self.frame_at(s0, true);
        stations.push(a.point);
        top.push(a.top);
        roll.push(a.roll);
        for i in 1..self.stations.len() - 1 {
            let c = self.cumulative[i];
            if c > s0 + TOL && c < s1 - TOL {
                stations.push(self.stations[i]);
                top.push(self.top[i]);
                roll.push(self.roll[i]);
            }
        }
        let b = self.frame_at(s1, false);
        stations.push(b.point);
        top.push(b.top);
        roll.push(b.roll);
        Spine::new(stations, top, roll, self.half_width, self.ditch)
    }

    pub fn nearest(&self, p: DVec2) -> Nearest {
        let mut best: Option<Nearest> = None;
        for seg in 0..self.segments() {
            let cand = self.nearest_in(seg, p);
            if best.is_none_or(|b| cand.dist < b.dist) {
                best = Some(cand);
            }
        }
        best.unwrap_or(Nearest {
            s: 0.0,
            point: self.start(),
            lateral: 0.0,
            dist: p.distance(self.start()),
            segment: 0,
        })
    }

    pub fn nearest_in(&self, seg: usize, p: DVec2) -> Nearest {
        let a = self.stations[seg];
        let b = self.stations[seg + 1];
        let d = b - a;
        let len2 = d.length_squared();
        let f = if len2 > TOL { ((p - a).dot(d) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let on = a + d * f;
        let dir = self.chord_dir(seg);
        let off = p - on;
        let dist = off.length();
        let lateral = if dist < TOL { 0.0 } else { off.dot(perp_left(dir)) };
        Nearest { s: self.cumulative[seg] + len2.sqrt() * f, point: on, lateral, dist, segment: seg }
    }

    pub fn bounds(&self, grow: f64) -> (DVec2, DVec2) {
        let mut lo = DVec2::splat(f64::INFINITY);
        let mut hi = DVec2::splat(f64::NEG_INFINITY);
        for p in &self.stations {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        (lo - DVec2::splat(grow), hi + DVec2::splat(grow))
    }
}
