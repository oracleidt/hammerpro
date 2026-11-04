use glam::DVec3;

use crate::terrain::{HeightGraph, HydraulicParams, ThermalParams, mix64, perlin};

#[derive(Clone, Debug, PartialEq)]
pub struct HeightField {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f64>,
}

impl HeightField {
    pub fn new(width: usize, height: usize, data: Vec<f64>) -> Option<Self> {
        (width >= 1 && height >= 1 && data.len() == width.checked_mul(height)?).then_some(Self { width, height, data })
    }

    pub fn filled(width: usize, height: usize, value: f64) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        Self { width, height, data: vec![value; width * height] }
    }

    pub fn at(&self, x: isize, y: isize) -> f64 {
        let cx = x.clamp(0, self.width as isize - 1) as usize;
        let cy = y.clamp(0, self.height as isize - 1) as usize;
        self.data.get(cy * self.width + cx).copied().unwrap_or(0.0)
    }

    pub fn min_max(&self) -> (f64, f64) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for v in &self.data {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        if lo > hi { (0.0, 0.0) } else { (lo, hi) }
    }

    pub fn sample(&self, x: f64, y: f64) -> f64 {
        if !x.is_finite() || !y.is_finite() {
            return 0.0;
        }
        let x = x.clamp(0.0, (self.width - 1) as f64);
        let y = y.clamp(0.0, (self.height - 1) as f64);
        let (ix, iy) = (x.floor() as isize, y.floor() as isize);
        let (fx, fy) = (x - ix as f64, y - iy as f64);
        let (lo, hi) = {
            let mut lo = f64::INFINITY;
            let mut hi = f64::NEG_INFINITY;
            for yy in [iy, iy + 1] {
                for xx in [ix, ix + 1] {
                    let v = self.at(xx, yy);
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            }
            (lo, hi)
        };
        let row = |yy: isize| {
            let p: [f64; 4] = std::array::from_fn(|i| self.extended(ix + i as isize - 1, yy));
            catmull(p, fx)
        };
        let rows: [f64; 4] = std::array::from_fn(|j| {
            let yy = iy + j as isize - 1;
            let last = self.height as isize - 1;
            if yy < 0 {
                2.0 * row(0) - row(1.min(last))
            } else if yy > last {
                2.0 * row(last) - row((last - 1).max(0))
            } else {
                row(yy)
            }
        });
        catmull(rows, fy).clamp(lo, hi)
    }

    fn extended(&self, x: isize, y: isize) -> f64 {
        let last = self.width as isize - 1;
        if x < 0 {
            2.0 * self.at(0, y) - self.at(1.min(last), y)
        } else if x > last {
            2.0 * self.at(last, y) - self.at((last - 1).max(0), y)
        } else {
            self.at(x, y)
        }
    }

    pub fn sample_unit(&self, u: f64, v: f64) -> f64 {
        self.sample(u * (self.width - 1) as f64, v * (self.height - 1) as f64)
    }

    pub fn sample_linear(&self, x: f64, y: f64) -> f64 {
        if !x.is_finite() || !y.is_finite() {
            return 0.0;
        }
        let x = x.clamp(0.0, (self.width - 1) as f64);
        let y = y.clamp(0.0, (self.height - 1) as f64);
        let (ix, iy) = (x.floor() as isize, y.floor() as isize);
        let (fx, fy) = (x - ix as f64, y - iy as f64);
        let top = self.at(ix, iy) * (1.0 - fx) + self.at(ix + 1, iy) * fx;
        let bottom = self.at(ix, iy + 1) * (1.0 - fx) + self.at(ix + 1, iy + 1) * fx;
        top * (1.0 - fy) + bottom * fy
    }

    pub fn sample_linear_unit(&self, u: f64, v: f64) -> f64 {
        self.sample_linear(u * (self.width - 1) as f64, v * (self.height - 1) as f64)
    }

    pub fn resampled(&self, width: usize, height: usize) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        let mut data = Vec::with_capacity(width * height);
        for j in 0..height {
            for i in 0..width {
                let u = if width > 1 { i as f64 / (width - 1) as f64 } else { 0.0 };
                let v = if height > 1 { j as f64 / (height - 1) as f64 } else { 0.0 };
                data.push(self.sample_unit(u, v));
            }
        }
        Self { width, height, data }
    }

    pub fn invert(&mut self) {
        for v in &mut self.data {
            *v = 1.0 - *v;
        }
    }

    pub fn flip_x(&mut self) {
        for row in self.data.chunks_mut(self.width) {
            row.reverse();
        }
    }

    pub fn flip_y(&mut self) {
        let w = self.width;
        let rows: Vec<Vec<f64>> = self.data.chunks(w).rev().map(<[f64]>::to_vec).collect();
        self.data = rows.concat();
    }

    pub fn smooth(&mut self, passes: u32) {
        for _ in 0..passes.min(64) {
            let src = self.clone();
            for y in 0..self.height as isize {
                for x in 0..self.width as isize {
                    let mut sum = 0.0;
                    let mut weight = 0.0;
                    for dy in -1..=1isize {
                        for dx in -1..=1isize {
                            let w = if dx == 0 && dy == 0 {
                                4.0
                            } else if dx == 0 || dy == 0 {
                                2.0
                            } else {
                                1.0
                            };
                            sum += w * src.at(x + dx, y + dy);
                            weight += w;
                        }
                    }
                    if let Some(v) = self.data.get_mut(y as usize * self.width + x as usize) {
                        *v = sum / weight;
                    }
                }
            }
        }
    }

    pub fn erode(
        &mut self,
        units: f64,
        cell: (f64, f64),
        thermal: Option<&ThermalParams>,
        hydraulic: Option<&HydraulicParams>,
    ) {
        if (thermal.is_none() && hydraulic.is_none()) || !units.is_finite() || units <= 0.0 {
            return;
        }
        let (w, h) = (self.width, self.height);
        let cell = (cell.0.max(1e-6), cell.1.max(1e-6));
        let mut links: Vec<Vec<(usize, f64)>> = vec![Vec::new(); w * h];
        for y in 0..h {
            for x in 0..w {
                if let Some(list) = links.get_mut(y * w + x) {
                    if x > 0 {
                        list.push((y * w + x - 1, cell.0));
                    }
                    if x + 1 < w {
                        list.push((y * w + x + 1, cell.0));
                    }
                    if y > 0 {
                        list.push(((y - 1) * w + x, cell.1));
                    }
                    if y + 1 < h {
                        list.push(((y + 1) * w + x, cell.1));
                    }
                }
            }
        }
        let mut graph = HeightGraph::new(self.data.iter().map(|v| v * units).collect(), links);
        if let Some(p) = thermal {
            graph.thermal(p);
        }
        if let Some(p) = hydraulic {
            graph.hydraulic(p);
        }
        for (v, g) in self.data.iter_mut().zip(&graph.heights) {
            *v = g / units;
        }
    }

    pub fn gradient(&self, x: isize, y: isize, cell: (f64, f64), units: f64) -> (f64, f64) {
        let dx = (self.at(x + 1, y) - self.at(x - 1, y)) * units / (2.0 * cell.0.max(1e-9));
        let dy = (self.at(x, y + 1) - self.at(x, y - 1)) * units / (2.0 * cell.1.max(1e-9));
        (dx, dy)
    }
}

fn catmull(p: [f64; 4], t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * (2.0 * p[1]
        + (-p[0] + p[2]) * t
        + (2.0 * p[0] - 5.0 * p[1] + 4.0 * p[2] - p[3]) * t2
        + (-p[0] + 3.0 * p[1] - 3.0 * p[2] + p[3]) * t3)
}

const F2: f64 = 0.366_025_403_784_438_6;
const G2: f64 = 0.211_324_865_405_187_1;
const GRADS: [[f64; 2]; 8] =
    [[1.0, 1.0], [-1.0, 1.0], [1.0, -1.0], [-1.0, -1.0], [1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]];

pub fn simplex2(x: f64, y: f64, seed: u64) -> f64 {
    if !x.is_finite() || !y.is_finite() {
        return 0.0;
    }
    let s = (x + y) * F2;
    let (i, j) = ((x + s).floor(), (y + s).floor());
    let t = (i + j) * G2;
    let (x0, y0) = (x - (i - t), y - (j - t));
    let (i1, j1) = if x0 > y0 { (1.0, 0.0) } else { (0.0, 1.0) };
    let corner = |dx: f64, dy: f64, gi: f64, gj: f64| {
        let falloff = 0.5 - dx * dx - dy * dy;
        if falloff <= 0.0 {
            return 0.0;
        }
        let h = mix64(seed ^ mix64((gi as i64 as u64) ^ mix64(gj as i64 as u64)));
        let g = GRADS.get((h % 8) as usize).copied().unwrap_or([0.0; 2]);
        let f2 = falloff * falloff;
        f2 * f2 * (g[0] * dx + g[1] * dy)
    };
    let n0 = corner(x0, y0, i, j);
    let n1 = corner(x0 - i1 + G2, y0 - j1 + G2, i + i1, j + j1);
    let n2 = corner(x0 - 1.0 + 2.0 * G2, y0 - 1.0 + 2.0 * G2, i + 1.0, j + 1.0);
    ((n0 + n1 + n2) * 45.0).clamp(-1.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseBasis {
    Perlin,
    Simplex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoiseShape {
    Plain,
    Ridged,
    Billow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FalloffShape {
    Round,
    Square,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainPreset {
    RollingHills,
    Mountains,
    Canyon,
    Islands,
    Dunes,
    Plateau,
}

impl TerrainPreset {
    pub const ALL: [Self; 6] =
        [Self::RollingHills, Self::Mountains, Self::Canyon, Self::Islands, Self::Dunes, Self::Plateau];

    pub fn name(self) -> &'static str {
        match self {
            Self::RollingHills => "rolling_hills",
            Self::Mountains => "mountains",
            Self::Canyon => "canyon",
            Self::Islands => "islands",
            Self::Dunes => "dunes",
            Self::Plateau => "plateau",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GenParams {
    pub basis: NoiseBasis,
    pub shape: NoiseShape,
    pub frequency: f64,
    pub octaves: u32,
    pub persistence: f64,
    pub warp: f64,
    pub stretch: f64,
    pub contrast: f64,
    pub exponent: f64,
    pub invert: bool,
    pub terraces: u32,
    pub terrace_sharpness: f64,
    pub sea_level: f64,
    pub island: bool,
    pub falloff_shape: FalloffShape,
    pub falloff: f64,
    pub falloff_power: f64,
    pub seed: u64,
}

impl Default for GenParams {
    fn default() -> Self {
        Self::preset(TerrainPreset::RollingHills, 1)
    }
}

impl GenParams {
    pub fn preset(preset: TerrainPreset, seed: u64) -> Self {
        let base = Self {
            basis: NoiseBasis::Simplex,
            shape: NoiseShape::Plain,
            frequency: 3.0,
            octaves: 4,
            persistence: 0.45,
            warp: 0.0,
            stretch: 1.0,
            contrast: 1.4,
            exponent: 1.0,
            invert: false,
            terraces: 0,
            terrace_sharpness: 0.5,
            sea_level: 0.0,
            island: false,
            falloff_shape: FalloffShape::Round,
            falloff: 0.5,
            falloff_power: 1.0,
            seed,
        };
        match preset {
            TerrainPreset::RollingHills => Self { warp: 0.1, exponent: 1.2, ..base },
            TerrainPreset::Mountains => Self {
                basis: NoiseBasis::Perlin,
                shape: NoiseShape::Ridged,
                octaves: 6,
                persistence: 0.5,
                warp: 0.15,
                exponent: 1.3,
                ..base
            },
            TerrainPreset::Canyon => Self {
                shape: NoiseShape::Ridged,
                frequency: 2.5,
                octaves: 5,
                warp: 0.2,
                invert: true,
                terraces: 4,
                terrace_sharpness: 0.6,
                ..base
            },
            TerrainPreset::Islands => Self {
                frequency: 4.0,
                octaves: 5,
                persistence: 0.5,
                exponent: 1.1,
                sea_level: 0.35,
                island: true,
                falloff: 0.6,
                falloff_power: 1.5,
                ..base
            },
            TerrainPreset::Dunes => Self {
                shape: NoiseShape::Ridged,
                frequency: 5.0,
                octaves: 3,
                persistence: 0.35,
                warp: 0.25,
                stretch: 3.0,
                ..base
            },
            TerrainPreset::Plateau => Self {
                basis: NoiseBasis::Perlin,
                frequency: 2.0,
                octaves: 5,
                warp: 0.1,
                terraces: 2,
                terrace_sharpness: 0.85,
                sea_level: 0.2,
                ..base
            },
        }
    }

    fn basis_at(&self, x: f64, y: f64, seed: u64) -> f64 {
        match self.basis {
            NoiseBasis::Perlin => (perlin(DVec3::new(x, y, 0.37), seed) * 1.35).clamp(-1.0, 1.0),
            NoiseBasis::Simplex => simplex2(x, y, seed),
        }
    }

    fn raw(&self, x: f64, y: f64) -> f64 {
        let seed = self.seed;
        let y = y / self.stretch.clamp(0.1, 16.0);
        let (mut x, mut y) = (x, y);
        let warp = if self.warp.is_finite() { self.warp.clamp(0.0, 2.0) } else { 0.0 };
        if warp > 0.0 {
            let wx = self.basis_at(x * 0.7 + 17.3, y * 0.7 + 3.1, mix64(seed ^ 0x57a3));
            let wy = self.basis_at(x * 0.7 - 9.7, y * 0.7 + 41.9, mix64(seed ^ 0x91c7));
            x += wx * warp * 2.0;
            y += wy * warp * 2.0;
        }
        let octaves = self.octaves.clamp(1, 10);
        let persistence = if self.persistence.is_finite() { self.persistence.clamp(0.05, 1.0) } else { 0.5 };
        let (mut sum, mut norm, mut amp, mut freq) = (0.0, 0.0, 1.0, 1.0);
        for o in 0..octaves {
            let n = self.basis_at(x * freq, y * freq, mix64(seed ^ u64::from(o)));
            let n = match self.shape {
                NoiseShape::Plain => n,
                NoiseShape::Ridged => 1.0 - 2.0 * n.abs(),
                NoiseShape::Billow => 2.0 * n.abs() - 1.0,
            };
            sum += amp * n;
            norm += amp;
            amp *= persistence;
            freq *= 2.0;
        }
        let contrast = if self.contrast.is_finite() { self.contrast.clamp(0.1, 4.0) } else { 1.0 };
        let v = (0.5 + 0.5 * (sum / norm * contrast).clamp(-1.0, 1.0)).clamp(0.0, 1.0);
        if self.invert { 1.0 - v } else { v }
    }

    pub fn value(&self, u: f64, v: f64, aspect: (f64, f64)) -> f64 {
        let m = aspect.0.max(aspect.1).max(1e-9);
        let freq = if self.frequency.is_finite() { self.frequency.clamp(0.1, 64.0) } else { 3.0 };
        let mut h = self.raw(u * aspect.0 / m * freq, v * aspect.1 / m * freq);
        let exponent = if self.exponent.is_finite() { self.exponent.clamp(0.2, 5.0) } else { 1.0 };
        h = h.powf(exponent);
        let sea = self.sea_level.clamp(0.0, 0.95);
        if sea > 0.0 {
            h = ((h - sea) / (1.0 - sea)).max(0.0);
        }
        if self.terraces > 0 {
            let n = f64::from(self.terraces.min(64));
            let t = h.clamp(0.0, 1.0) * n;
            let step = t.floor().min(n - 1.0);
            let f = t - step;
            let width = (1.0 - 0.9 * self.terrace_sharpness.clamp(0.0, 1.0)).clamp(0.1, 1.0);
            let e0 = 0.5 - width / 2.0;
            let s = ((f - e0) / width).clamp(0.0, 1.0);
            h = (step + s * s * (3.0 - 2.0 * s)) / n;
        }
        if self.island {
            let (dx, dy) = (2.0 * u - 1.0, 2.0 * v - 1.0);
            let d = match self.falloff_shape {
                FalloffShape::Round => (dx * dx + dy * dy).sqrt(),
                FalloffShape::Square => dx.abs().max(dy.abs()),
            };
            let inner = 1.0 - self.falloff.clamp(0.0, 1.0);
            let t = if inner >= 1.0 { 0.0 } else { ((d - inner) / (1.0 - inner)).clamp(0.0, 1.0) };
            let mask = 1.0 - t * t * (3.0 - 2.0 * t);
            h *= mask.powf(self.falloff_power.clamp(0.1, 8.0));
        }
        h.clamp(0.0, 1.0)
    }
}

pub fn generate_field(params: &GenParams, width: usize, height: usize, size: (f64, f64)) -> HeightField {
    let (width, height) = (width.max(2), height.max(2));
    let mut data = Vec::with_capacity(width * height);
    for j in 0..height {
        for i in 0..width {
            data.push(params.value(i as f64 / (width - 1) as f64, j as f64 / (height - 1) as f64, size));
        }
    }
    HeightField { width, height, data }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AutoTexture {
    pub rock_slope: f64,
    pub rock_blend: f64,
    pub snow_height: f64,
    pub snow_blend: f64,
    pub sand_height: f64,
    pub sand_blend: f64,
}

impl Default for AutoTexture {
    fn default() -> Self {
        Self {
            rock_slope: 35.0,
            rock_blend: 10.0,
            snow_height: 0.8,
            snow_blend: 0.1,
            sand_height: 0.0,
            sand_blend: 0.05,
        }
    }
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    if e1 <= e0 {
        return if x >= e1 { 1.0 } else { 0.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn slope_degrees(dx: f64, dy: f64) -> f64 {
    (dx * dx + dy * dy).sqrt().atan().to_degrees()
}

pub fn auto_weights(slope_deg: f64, height01: f64, tex: &AutoTexture) -> [f64; 4] {
    let rock = smoothstep(tex.rock_slope, tex.rock_slope + tex.rock_blend.max(0.01), slope_deg);
    let snow = smoothstep(tex.snow_height, tex.snow_height + tex.snow_blend.max(0.001), height01) * (1.0 - rock);
    let sand = if tex.sand_height > 0.0 {
        (1.0 - smoothstep(tex.sand_height - tex.sand_blend.max(0.001), tex.sand_height, height01)) * (1.0 - rock)
    } else {
        0.0
    };
    let grass = (1.0 - rock - snow - sand).max(0.0);
    let sum = grass + rock + snow + sand;
    if sum <= 1e-9 { [1.0, 0.0, 0.0, 0.0] } else { [grass / sum, rock / sum, snow / sum, sand / sum] }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use super::*;

    #[test]
    fn bicubic_sampling_of_a_ramp_is_a_ramp() {
        let data: Vec<f64> = (0..8 * 4).map(|k| (k % 8) as f64 / 7.0).collect();
        let f = HeightField::new(8, 4, data).unwrap();
        for k in 0..=20 {
            let u = f64::from(k) / 20.0;
            assert!((f.sample_unit(u, 0.5) - u).abs() < 1e-9, "{u}");
        }
        assert_eq!(f.sample_unit(0.0, 0.0), 0.0);
        assert_eq!(f.sample_unit(1.0, 1.0), 1.0);
    }

    #[test]
    fn bicubic_never_overshoots_a_step() {
        let mut data = vec![0.0; 16];
        for row in data.chunks_mut(4) {
            row[2] = 1.0;
            row[3] = 1.0;
        }
        let f = HeightField::new(4, 4, data).unwrap();
        for k in 0..=60 {
            let v = f.sample(f64::from(k) / 20.0, 1.5);
            assert!((0.0..=1.0).contains(&v), "{v}");
        }
    }

    #[test]
    fn flips_and_invert_work() {
        let mut f = HeightField::new(3, 2, vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5]).unwrap();
        f.flip_x();
        assert_eq!(f.data, vec![0.2, 0.1, 0.0, 0.5, 0.4, 0.3]);
        f.flip_y();
        assert_eq!(f.data, vec![0.5, 0.4, 0.3, 0.2, 0.1, 0.0]);
        f.invert();
        assert!((f.data[0] - 0.5).abs() < 1e-12);
    }

    #[test]
    fn simplex_is_bounded_deterministic_and_varies() {
        let mut lo = 1.0f64;
        let mut hi = -1.0f64;
        for i in 0..400 {
            let (x, y) = (f64::from(i) * 0.37, f64::from(i % 17) * 0.53);
            let v = simplex2(x, y, 5);
            assert!((-1.0..=1.0).contains(&v));
            assert_eq!(v, simplex2(x, y, 5));
            lo = lo.min(v);
            hi = hi.max(v);
        }
        assert!(hi - lo > 0.8, "{lo} {hi}");
    }

    #[test]
    fn generation_is_deterministic_seeded_and_in_range() {
        for preset in TerrainPreset::ALL {
            let a = generate_field(&GenParams::preset(preset, 7), 17, 17, (1024.0, 1024.0));
            let b = generate_field(&GenParams::preset(preset, 7), 17, 17, (1024.0, 1024.0));
            let c = generate_field(&GenParams::preset(preset, 8), 17, 17, (1024.0, 1024.0));
            assert_eq!(a, b, "{preset:?}");
            assert_ne!(a.data, c.data, "{preset:?}");
            let (lo, hi) = a.min_max();
            assert!(lo >= 0.0 && hi <= 1.0 && hi - lo > 0.05, "{preset:?} {lo} {hi}");
        }
    }

    #[test]
    fn island_mask_drops_the_edges_to_zero() {
        let p =
            GenParams { island: true, falloff: 0.8, sea_level: 0.0, ..GenParams::preset(TerrainPreset::Islands, 3) };
        let f = generate_field(&p, 33, 33, (2048.0, 2048.0));
        for k in 0..33 {
            for (x, y) in [(k, 0), (k, 32), (0, k), (32, k)] {
                assert_eq!(f.at(x, y), 0.0);
            }
        }
        assert!(f.at(16, 16) > 0.0);
    }

    #[test]
    fn terraces_make_flat_steps() {
        let p = GenParams { terraces: 4, terrace_sharpness: 1.0, ..GenParams::preset(TerrainPreset::Mountains, 2) };
        let f = generate_field(&p, 33, 33, (1024.0, 1024.0));
        let steps: std::collections::HashSet<i64> = f.data.iter().map(|v| (v * 4.0).round() as i64).collect();
        assert!(steps.len() <= 5);
    }

    #[test]
    fn erosion_changes_the_field_and_keeps_the_scale() {
        let p = GenParams::preset(TerrainPreset::Mountains, 4);
        let mut f = generate_field(&p, 33, 33, (2048.0, 2048.0));
        let before = f.clone();
        f.erode(512.0, (64.0, 64.0), Some(&ThermalParams { iterations: 30, talus_degrees: 30.0, strength: 1.0 }), None);
        assert_ne!(f, before);
        let sum_a: f64 = before.data.iter().sum();
        let sum_b: f64 = f.data.iter().sum();
        assert!((sum_a - sum_b).abs() < 1e-6);
    }

    #[test]
    fn auto_weights_follow_slope_and_height() {
        let tex = AutoTexture::default();
        let flat = auto_weights(0.0, 0.3, &tex);
        assert!(flat[0] > 0.99);
        let steep = auto_weights(70.0, 0.3, &tex);
        assert!(steep[1] > 0.99);
        let high = auto_weights(0.0, 0.95, &tex);
        assert!(high[2] > 0.99);
        let steep_high = auto_weights(70.0, 0.95, &tex);
        assert!(steep_high[1] > 0.99);
        let tex = AutoTexture { sand_height: 0.2, ..tex };
        assert!(auto_weights(0.0, 0.05, &tex)[3] > 0.9);
        for w in [flat, steep, high, auto_weights(40.0, 0.5, &tex)] {
            assert!((w.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        }
    }
}
