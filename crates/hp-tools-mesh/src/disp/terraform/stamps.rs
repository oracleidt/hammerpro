use std::path::Path;

use glam::DVec3;
use hp_geom::perlin;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StampShape {
    #[default]
    Mountain,
    Hill,
    Crater,
    Mesa,
    Volcano,
    Dune,
    Ridge,
    Rock,
    Image,
}

impl StampShape {
    pub const ALL: [StampShape; 9] = [
        StampShape::Mountain,
        StampShape::Hill,
        StampShape::Crater,
        StampShape::Mesa,
        StampShape::Volcano,
        StampShape::Dune,
        StampShape::Ridge,
        StampShape::Rock,
        StampShape::Image,
    ];

    pub fn name(self) -> &'static str {
        match self {
            StampShape::Mountain => "mountain",
            StampShape::Hill => "hill",
            StampShape::Crater => "crater",
            StampShape::Mesa => "mesa",
            StampShape::Volcano => "volcano",
            StampShape::Dune => "dune",
            StampShape::Ridge => "ridge",
            StampShape::Rock => "rock",
            StampShape::Image => "image",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "plateau" => Some(StampShape::Mesa),
            "ridge_line" => Some(StampShape::Ridge),
            "outcrop" | "rock_outcrop" => Some(StampShape::Rock),
            _ => StampShape::ALL.into_iter().find(|s| s.name() == name),
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            StampShape::Mountain => "tf.stamp.mountain",
            StampShape::Hill => "tf.stamp.hill",
            StampShape::Crater => "tf.stamp.crater",
            StampShape::Mesa => "tf.stamp.mesa",
            StampShape::Volcano => "tf.stamp.volcano",
            StampShape::Dune => "tf.stamp.dune",
            StampShape::Ridge => "tf.stamp.ridge",
            StampShape::Rock => "tf.stamp.rock",
            StampShape::Image => "tf.stamp.image",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StampImage {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f64>,
}

impl StampImage {
    pub fn new(width: usize, height: usize, mut data: Vec<f64>) -> Option<Self> {
        if width < 2 || height < 2 || data.len() != width.checked_mul(height)? {
            return None;
        }
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for v in &data {
            if v.is_finite() {
                lo = lo.min(*v);
                hi = hi.max(*v);
            }
        }
        let span = hi - lo;
        for v in &mut data {
            *v = if v.is_finite() && span > 1e-12 { (*v - lo) / span } else { 0.0 };
        }
        Some(Self { width, height, data })
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        let field = crate::terrain::image_io::load_gray(path)?;
        Self::new(field.width, field.height, field.data).ok_or_else(|| "the image is too small".to_owned())
    }

    pub fn sample(&self, u: f64, v: f64) -> f64 {
        let x = ((u + 1.0) * 0.5 * (self.width - 1) as f64).clamp(0.0, (self.width - 1) as f64);
        let y = ((1.0 - (v + 1.0) * 0.5) * (self.height - 1) as f64).clamp(0.0, (self.height - 1) as f64);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let at = |cx: usize, cy: usize| self.data.get(cy * self.width + cx).copied().unwrap_or(0.0);
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
        top + (bottom - top) * fy
    }
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn detail(u: f64, v: f64, freq: f64, seed: u64) -> f64 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut f = freq;
    let mut norm = 0.0;
    for i in 0..4u64 {
        sum += amp * perlin(DVec3::new(u * f, v * f, 0.37), seed.wrapping_add(i * 977));
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    sum / norm
}

fn ridged(u: f64, v: f64, freq: f64, seed: u64) -> f64 {
    let mut sum = 0.0;
    let mut amp = 1.0;
    let mut f = freq;
    let mut norm = 0.0;
    for i in 0..4u64 {
        let n = 1.0 - perlin(DVec3::new(u * f, v * f, 0.71), seed.wrapping_add(i * 131)).abs();
        sum += amp * n * n;
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    sum / norm
}

pub fn shape_value(shape: StampShape, image: Option<&StampImage>, u: f64, v: f64) -> f64 {
    let r = (u * u + v * v).sqrt();
    if !r.is_finite() {
        return 0.0;
    }
    let round_edge = smoothstep(1.0, 0.92, r);
    match shape {
        StampShape::Mountain => {
            if r >= 1.0 {
                return 0.0;
            }
            let base = (1.0 - smoothstep(0.0, 1.0, r)).powf(1.4);
            let ridges = ridged(u, v, 2.2, 11);
            (base * (0.62 + 0.38 * ridges) + 0.05 * base * detail(u, v, 6.0, 5)).max(0.0) * round_edge
        }
        StampShape::Hill => {
            if r >= 1.0 {
                return 0.0;
            }
            0.5 * (1.0 + (std::f64::consts::PI * r).cos())
        }
        StampShape::Crater => {
            if r >= 1.0 {
                return 0.0;
            }
            let bowl = -(1.0 - (r / 0.62).powi(2)).max(0.0) * 0.75;
            let rim = (-((r - 0.64) / 0.13).powi(2)).exp() * 0.45;
            let ejecta = (1.0 - smoothstep(0.64, 1.0, r)) * 0.08 * (1.0 + detail(u, v, 5.0, 23));
            (bowl + rim + ejecta * smoothstep(0.55, 0.7, r)) * round_edge
        }
        StampShape::Mesa => {
            let top = 1.0 - smoothstep(0.5, 0.66, r + 0.03 * detail(u, v, 4.0, 3));
            let skirt = 0.18 * (1.0 - smoothstep(0.62, 1.0, r));
            top.max(skirt) * round_edge
        }
        StampShape::Volcano => {
            if r >= 1.0 {
                return 0.0;
            }
            let cone = (1.0 - r).powf(1.35);
            let caldera = 0.3 * (1.0 - smoothstep(0.0, 0.16, r));
            let gullies = 0.06 * (1.0 - r) * ridged(u, v, 3.0, 41);
            (cone - caldera + gullies).max(0.0) * round_edge
        }
        StampShape::Dune => {
            let w = u + 0.35 * v * v;
            let crest = 0.25;
            let profile =
                if w < crest { smoothstep(-1.0, crest, w).powf(1.3) } else { 1.0 - smoothstep(crest, crest + 0.28, w) };
            let lateral = (1.0 - v * v).max(0.0);
            profile * lateral * round_edge
        }
        StampShape::Ridge => {
            let a = v.abs() / 0.42;
            if a >= 1.0 {
                return 0.0;
            }
            let lateral = (1.0 - a).powf(1.5);
            let along = 1.0 - smoothstep(0.55, 1.0, u.abs());
            let top = 0.75 + 0.25 * detail(u, 0.0, 3.0, 17);
            (lateral * along * top + 0.05 * lateral * detail(u, v, 8.0, 9)).max(0.0)
        }
        StampShape::Rock => {
            if r >= 1.0 {
                return 0.0;
            }
            let base = 1.0 - smoothstep(0.15, 0.95, r);
            let n = detail(u, v, 2.5, 61);
            let raw = (base * 0.85 + 0.3 * n * base).max(0.0);
            let steps = 5.0;
            let stepped = (raw * steps).floor() / steps;
            let blend = raw - stepped;
            (stepped + blend * 0.25).max(0.0) * round_edge
        }
        StampShape::Image => {
            let Some(img) = image else { return 0.0 };
            let edge = u.abs().max(v.abs());
            if edge >= 1.0 {
                return 0.0;
            }
            img.sample(u, v) * smoothstep(1.0, 0.94, edge)
        }
    }
}

pub fn local_uv(d_u: f64, d_v: f64, size: f64, rotation_deg: f64) -> (f64, f64) {
    let (s, c) = (-rotation_deg.to_radians()).sin_cos();
    let (u, v) = (d_u / size, d_v / size);
    (u * c - v * s, u * s + v * c)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::float_cmp)]

    use super::*;

    #[test]
    fn shapes_vanish_outside_the_footprint() {
        for shape in StampShape::ALL {
            assert_eq!(shape_value(shape, None, 1.2, 0.0), 0.0, "{shape:?}");
            assert_eq!(shape_value(shape, None, 0.0, -1.05), 0.0, "{shape:?}");
        }
    }

    #[test]
    fn mountain_peaks_in_the_middle_and_crater_dips() {
        let peak = shape_value(StampShape::Mountain, None, 0.0, 0.0);
        let flank = shape_value(StampShape::Mountain, None, 0.7, 0.0);
        assert!(peak > 0.5 && flank < peak);
        assert!(shape_value(StampShape::Crater, None, 0.0, 0.0) < 0.0);
        assert!(shape_value(StampShape::Crater, None, 0.64, 0.0) > 0.2);
        assert!(shape_value(StampShape::Mesa, None, 0.2, 0.1) > 0.95);
    }

    #[test]
    fn rotation_turns_the_local_frame() {
        let (u, v) = local_uv(0.0, 10.0, 10.0, 90.0);
        assert!((u - 1.0).abs() < 1e-9 && v.abs() < 1e-9);
        let img = StampImage::new(2, 2, vec![0.0, 1.0, 0.0, 1.0]).unwrap();
        assert!(img.sample(0.9, 0.0) > img.sample(-0.9, 0.0));
    }
}
