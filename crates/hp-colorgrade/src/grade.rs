use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Channel {
    pub in_black: f32,
    pub in_white: f32,
    pub gamma: f32,
    pub out_black: f32,
    pub out_white: f32,
}

impl Default for Channel {
    fn default() -> Self {
        Self { in_black: 0.0, in_white: 1.0, gamma: 1.0, out_black: 0.0, out_white: 1.0 }
    }
}

impl Channel {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    fn apply(&self, v: f32) -> f32 {
        let span = (self.in_white - self.in_black).max(1e-4);
        let t = ((v - self.in_black) / span).clamp(0.0, 1.0);
        let t = t.powf(1.0 / self.gamma.clamp(0.1, 10.0));
        self.out_black + t * (self.out_white - self.out_black)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Curve {
    pub points: Vec<[f32; 2]>,
}

impl Curve {
    pub fn is_neutral(&self) -> bool {
        self.points.len() < 2 || self.points.iter().all(|p| (p[0] - p[1]).abs() < 1e-4)
    }

    pub fn table(&self) -> Vec<f32> {
        let mut pts: Vec<[f32; 2]> = self
            .points
            .iter()
            .filter(|p| p[0].is_finite() && p[1].is_finite())
            .map(|p| [p[0].clamp(0.0, 1.0), p[1].clamp(0.0, 1.0)])
            .collect();
        pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
        pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-5);
        if pts.first().is_none_or(|p| p[0] > 0.0) {
            let y = pts.first().map_or(0.0, |p| p[1]);
            pts.insert(0, [0.0, y]);
        }
        if pts.last().is_none_or(|p| p[0] < 1.0) {
            let y = pts.last().map_or(1.0, |p| p[1]);
            pts.push([1.0, y]);
        }
        let n = pts.len();
        let mut slope = vec![0.0_f32; n - 1];
        for i in 0..n - 1 {
            slope[i] = (pts[i + 1][1] - pts[i][1]) / (pts[i + 1][0] - pts[i][0]).max(1e-5);
        }
        let mut tangent = vec![0.0_f32; n];
        tangent[0] = slope[0];
        tangent[n - 1] = slope[n - 2];
        for i in 1..n - 1 {
            tangent[i] = if slope[i - 1] * slope[i] <= 0.0 { 0.0 } else { (slope[i - 1] + slope[i]) * 0.5 };
        }
        for i in 0..n - 1 {
            if slope[i].abs() < 1e-9 {
                tangent[i] = 0.0;
                tangent[i + 1] = 0.0;
            } else {
                let a = tangent[i] / slope[i];
                let b = tangent[i + 1] / slope[i];
                let h = a.hypot(b);
                if h > 3.0 {
                    let k = 3.0 / h;
                    tangent[i] = k * a * slope[i];
                    tangent[i + 1] = k * b * slope[i];
                }
            }
        }
        (0..TABLE)
            .map(|k| {
                let x = k as f32 / (TABLE - 1) as f32;
                let seg = pts.iter().rposition(|p| p[0] <= x).unwrap_or(0).min(n - 2);
                let (x0, x1) = (pts[seg][0], pts[seg + 1][0]);
                let h = (x1 - x0).max(1e-5);
                let t = ((x - x0) / h).clamp(0.0, 1.0);
                let (t2, t3) = (t * t, t * t * t);
                let y = (2.0 * t3 - 3.0 * t2 + 1.0) * pts[seg][1]
                    + (t3 - 2.0 * t2 + t) * h * tangent[seg]
                    + (-2.0 * t3 + 3.0 * t2) * pts[seg + 1][1]
                    + (t3 - t2) * h * tangent[seg + 1];
                y.clamp(0.0, 1.0)
            })
            .collect()
    }
}

const TABLE: usize = 256;

fn lookup(table: &[f32], v: f32) -> f32 {
    let x = v.clamp(0.0, 1.0) * (TABLE - 1) as f32;
    let i = x.floor() as usize;
    let j = (i + 1).min(TABLE - 1);
    let k = x - i as f32;
    table[i] * (1.0 - k) + table[j] * k
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tint {
    pub color: [f32; 3],
    pub amount: f32,
}

impl Default for Tint {
    fn default() -> Self {
        Self { color: [1.0, 1.0, 1.0], amount: 0.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Colorize {
    pub color: [f32; 3],
    pub amount: f32,
}

impl Default for Colorize {
    fn default() -> Self {
        Self { color: [1.0, 0.85, 0.6], amount: 0.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Grade {
    pub exposure: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub gamma: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub hue: f32,
    pub temperature: f32,
    pub magenta: f32,
    pub shadows: f32,
    pub highlights: f32,
    pub tint: Tint,
    pub colorize: Colorize,
    pub levels: [Channel; 4],
    pub curves: [Curve; 4],
    pub invert: bool,
}

impl Default for Grade {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            brightness: 0.0,
            contrast: 0.0,
            gamma: 1.0,
            saturation: 1.0,
            vibrance: 0.0,
            hue: 0.0,
            temperature: 0.0,
            magenta: 0.0,
            shadows: 0.0,
            highlights: 0.0,
            tint: Tint::default(),
            colorize: Colorize::default(),
            levels: [Channel::default(); 4],
            curves: Default::default(),
            invert: false,
        }
    }
}

pub const RGB_WEIGHTS: [f32; 3] = [0.2126, 0.7152, 0.0722];

fn luma(c: [f32; 3]) -> f32 {
    c[0] * RGB_WEIGHTS[0] + c[1] * RGB_WEIGHTS[1] + c[2] * RGB_WEIGHTS[2]
}

fn to_linear(v: f32) -> f32 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn to_srgb(v: f32) -> f32 {
    let v = v.max(0.0);
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Grade {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    pub fn clamped(&self) -> Self {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let mut g = self.clone();
        g.exposure = f(g.exposure, -5.0, 5.0, 0.0);
        g.brightness = f(g.brightness, -1.0, 1.0, 0.0);
        g.contrast = f(g.contrast, -1.0, 1.0, 0.0);
        g.gamma = f(g.gamma, 0.1, 5.0, 1.0);
        g.saturation = f(g.saturation, 0.0, 4.0, 1.0);
        g.vibrance = f(g.vibrance, -1.0, 1.0, 0.0);
        g.hue = f(g.hue, -180.0, 180.0, 0.0);
        g.temperature = f(g.temperature, -1.0, 1.0, 0.0);
        g.magenta = f(g.magenta, -1.0, 1.0, 0.0);
        g.shadows = f(g.shadows, -1.0, 1.0, 0.0);
        g.highlights = f(g.highlights, -1.0, 1.0, 0.0);
        g.tint.amount = f(g.tint.amount, 0.0, 1.0, 0.0);
        g.colorize.amount = f(g.colorize.amount, 0.0, 1.0, 0.0);
        for c in g.tint.color.iter_mut().chain(g.colorize.color.iter_mut()) {
            *c = f(*c, 0.0, 1.0, 1.0);
        }
        for l in &mut g.levels {
            l.in_black = f(l.in_black, 0.0, 0.99, 0.0);
            l.in_white = f(l.in_white, l.in_black + 0.01, 1.0, 1.0);
            l.gamma = f(l.gamma, 0.1, 10.0, 1.0);
            l.out_black = f(l.out_black, 0.0, 1.0, 0.0);
            l.out_white = f(l.out_white, 0.0, 1.0, 1.0);
        }
        g
    }

    pub fn engine(&self) -> Engine {
        let g = self.clamped();
        let tables = [0, 1, 2, 3].map(|i| if g.curves[i].is_neutral() { None } else { Some(g.curves[i].table()) });
        Engine { g, tables }
    }
}

pub struct Engine {
    g: Grade,
    tables: [Option<Vec<f32>>; 4],
}

impl Engine {
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let g = &self.g;
        let mut c = c.map(|v| v.clamp(0.0, 1.0));
        if g.exposure != 0.0 {
            let k = 2.0_f32.powf(g.exposure);
            c = c.map(|v| to_srgb(to_linear(v) * k).min(4.0));
        }
        if g.temperature != 0.0 || g.magenta != 0.0 {
            c[0] *= 1.0 + 0.3 * g.temperature + 0.1 * g.magenta;
            c[1] *= 1.0 - 0.2 * g.magenta;
            c[2] *= 1.0 - 0.3 * g.temperature + 0.1 * g.magenta;
        }
        if g.brightness != 0.0 {
            c = c.map(|v| v + g.brightness);
        }
        if g.contrast != 0.0 {
            let k = if g.contrast >= 0.0 { 1.0 / (1.0 - g.contrast * 0.95) } else { 1.0 + g.contrast };
            c = c.map(|v| (v - 0.5) * k + 0.5);
        }
        if g.shadows != 0.0 || g.highlights != 0.0 {
            let l = luma(c.map(|v| v.clamp(0.0, 1.0)));
            let lift = g.shadows * 0.5 * (1.0 - smoothstep(0.0, 0.55, l));
            let gain = g.highlights * 0.5 * smoothstep(0.45, 1.0, l);
            c = c.map(|v| v + lift + gain);
        }
        c = c.map(|v| v.clamp(0.0, 1.0));
        if !g.levels[0].is_neutral() {
            c = c.map(|v| g.levels[0].apply(v));
        }
        for (i, v) in c.iter_mut().enumerate() {
            if !g.levels[i + 1].is_neutral() {
                *v = g.levels[i + 1].apply(*v);
            }
        }
        if let Some(t) = &self.tables[0] {
            c = c.map(|v| lookup(t, v));
        }
        for (i, v) in c.iter_mut().enumerate() {
            if let Some(t) = &self.tables[i + 1] {
                *v = lookup(t, *v);
            }
        }
        if g.gamma != 1.0 {
            c = c.map(|v| v.clamp(0.0, 1.0).powf(1.0 / g.gamma));
        }
        if g.hue != 0.0 {
            c = rotate_hue(c, g.hue.to_radians());
        }
        if g.saturation != 1.0 || g.vibrance != 0.0 {
            let l = luma(c);
            let mx = c[0].max(c[1]).max(c[2]);
            let mn = c[0].min(c[1]).min(c[2]);
            let current = if mx > 1e-5 { (mx - mn) / mx } else { 0.0 };
            let boost = g.saturation * (1.0 + g.vibrance * (1.0 - current));
            c = c.map(|v| l + (v - l) * boost.max(0.0));
        }
        if g.tint.amount > 0.0 {
            let a = g.tint.amount;
            for (v, t) in c.iter_mut().zip(g.tint.color) {
                *v = *v * (1.0 - a) + *v * t * a;
            }
        }
        if g.colorize.amount > 0.0 {
            let l = luma(c.map(|v| v.clamp(0.0, 1.0)));
            let tl = luma(g.colorize.color).max(1e-3);
            let a = g.colorize.amount;
            for (v, t) in c.iter_mut().zip(g.colorize.color) {
                *v = *v * (1.0 - a) + (l * t / tl) * a;
            }
        }
        if g.invert {
            c = c.map(|v| 1.0 - v.clamp(0.0, 1.0));
        }
        c.map(|v| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 })
    }
}

fn rotate_hue(c: [f32; 3], angle: f32) -> [f32; 3] {
    let (s, k) = angle.sin_cos();
    let y = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
    let i = 0.596 * c[0] - 0.274 * c[1] - 0.322 * c[2];
    let q = 0.211 * c[0] - 0.523 * c[1] + 0.312 * c[2];
    let i2 = i * k - q * s;
    let q2 = i * s + q * k;
    [y + 0.956 * i2 + 0.621 * q2, y - 0.272 * i2 - 0.647 * q2, y - 1.106 * i2 + 1.703 * q2]
}
