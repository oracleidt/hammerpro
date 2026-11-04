use glam::DVec3;

pub const MAX_OCTAVES: u32 = 8;
pub const MAX_ITERATIONS: u32 = 2000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NoiseParams {
    pub scale: f64,
    pub octaves: u32,
    pub persistence: f64,
    pub seed: u64,
}

impl Default for NoiseParams {
    fn default() -> Self {
        Self { scale: 256.0, octaves: 4, persistence: 0.5, seed: 1 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThermalParams {
    pub iterations: u32,
    pub talus_degrees: f64,
    pub strength: f64,
}

impl Default for ThermalParams {
    fn default() -> Self {
        Self { iterations: 20, talus_degrees: 35.0, strength: 0.5 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HydraulicParams {
    pub iterations: u32,
    pub rain: f64,
    pub evaporation: f64,
    pub capacity: f64,
    pub deposition: f64,
    pub strength: f64,
    pub seed: u64,
}

impl Default for HydraulicParams {
    fn default() -> Self {
        Self { iterations: 50, rain: 1.0, evaporation: 0.1, capacity: 1.0, deposition: 0.3, strength: 0.3, seed: 1 }
    }
}

pub(crate) fn mix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

pub(crate) fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

const GRADIENTS: [[f64; 3]; 12] = [
    [1.0, 1.0, 0.0],
    [-1.0, 1.0, 0.0],
    [1.0, -1.0, 0.0],
    [-1.0, -1.0, 0.0],
    [1.0, 0.0, 1.0],
    [-1.0, 0.0, 1.0],
    [1.0, 0.0, -1.0],
    [-1.0, 0.0, -1.0],
    [0.0, 1.0, 1.0],
    [0.0, -1.0, 1.0],
    [0.0, 1.0, -1.0],
    [0.0, -1.0, -1.0],
];

fn corner(seed: u64, ix: i64, iy: i64, iz: i64, d: DVec3) -> f64 {
    let h = mix64(seed ^ mix64(ix as u64 ^ mix64(iy as u64 ^ mix64(iz as u64))));
    let g = GRADIENTS.get((h % 12) as usize).copied().unwrap_or([0.0; 3]);
    g[0] * d.x + g[1] * d.y + g[2] * d.z
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

pub fn perlin(p: DVec3, seed: u64) -> f64 {
    if !p.is_finite() {
        return 0.0;
    }
    let f = p.floor();
    let (ix, iy, iz) = (f.x as i64, f.y as i64, f.z as i64);
    let r = p - f;
    let (u, v, w) = (fade(r.x), fade(r.y), fade(r.z));
    let c = |dx: i64, dy: i64, dz: i64| {
        corner(seed, ix + dx, iy + dy, iz + dz, r - DVec3::new(dx as f64, dy as f64, dz as f64))
    };
    let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
    let x00 = lerp(c(0, 0, 0), c(1, 0, 0), u);
    let x10 = lerp(c(0, 1, 0), c(1, 1, 0), u);
    let x01 = lerp(c(0, 0, 1), c(1, 0, 1), u);
    let x11 = lerp(c(0, 1, 1), c(1, 1, 1), u);
    lerp(lerp(x00, x10, v), lerp(x01, x11, v), w).clamp(-1.0, 1.0)
}

pub fn fractal_noise(p: DVec3, params: &NoiseParams) -> f64 {
    let scale = if params.scale.is_finite() && params.scale > 1e-6 { params.scale } else { 1.0 };
    let persistence = if params.persistence.is_finite() { params.persistence.clamp(0.0, 1.0) } else { 0.5 };
    let octaves = params.octaves.clamp(1, MAX_OCTAVES);
    let (mut sum, mut norm, mut amp, mut freq) = (0.0, 0.0, 1.0, 1.0);
    for octave in 0..octaves {
        sum += amp * perlin(p * (freq / scale), mix64(params.seed ^ u64::from(octave)));
        norm += amp;
        amp *= persistence;
        freq *= 2.0;
    }
    if norm > 0.0 { (sum / norm).clamp(-1.0, 1.0) } else { 0.0 }
}

#[derive(Clone, Debug, Default)]
pub struct HeightGraph {
    pub heights: Vec<f64>,
    pub links: Vec<Vec<(usize, f64)>>,
}

impl HeightGraph {
    pub fn new(heights: Vec<f64>, links: Vec<Vec<(usize, f64)>>) -> Self {
        let mut links = links;
        links.resize(heights.len(), Vec::new());
        for list in &mut links {
            list.retain(|(j, run)| *j < heights.len() && run.is_finite() && *run > 1e-9);
        }
        Self { heights, links }
    }

    pub fn total(&self) -> f64 {
        self.heights.iter().sum()
    }

    pub fn max_slope(&self) -> f64 {
        let mut best = 0.0f64;
        for (i, list) in self.links.iter().enumerate() {
            let Some(&hi) = self.heights.get(i) else { continue };
            for &(j, run) in list {
                if let Some(&hj) = self.heights.get(j) {
                    best = best.max((hi - hj).abs() / run);
                }
            }
        }
        best
    }

    pub fn thermal(&mut self, params: &ThermalParams) {
        let talus = params.talus_degrees.clamp(0.0, 89.9).to_radians().tan();
        let strength = if params.strength.is_finite() { params.strength.clamp(0.0, 1.0) } else { 0.0 };
        let n = self.heights.len();
        let mut delta = vec![0.0; n];
        for _ in 0..params.iterations.min(MAX_ITERATIONS) {
            delta.iter_mut().for_each(|d| *d = 0.0);
            for i in 0..n {
                let (Some(&hi), Some(list)) = (self.heights.get(i), self.links.get(i)) else { continue };
                let excess = |&(j, run): &(usize, f64)| {
                    let hj = self.heights.get(j).copied().unwrap_or(hi);
                    ((hi - hj) - talus * run).max(0.0)
                };
                let total: f64 = list.iter().map(excess).sum();
                if total <= 0.0 {
                    continue;
                }
                let biggest = list.iter().map(excess).fold(0.0, f64::max);
                let moved = strength * 0.5 * biggest;
                for link in list {
                    let share = moved * excess(link) / total;
                    if let Some(d) = delta.get_mut(i) {
                        *d -= share;
                    }
                    if let Some(d) = delta.get_mut(link.0) {
                        *d += share;
                    }
                }
            }
            for (h, d) in self.heights.iter_mut().zip(&delta) {
                *h += d;
            }
        }
    }

    pub fn hydraulic(&mut self, params: &HydraulicParams) {
        let clamp01 = |v: f64| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        let positive = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
        let (rain, capacity) = (positive(params.rain), positive(params.capacity));
        let (evaporation, deposition, strength) =
            (clamp01(params.evaporation), clamp01(params.deposition), clamp01(params.strength));
        let n = self.heights.len();
        let mut water = vec![0.0; n];
        let mut sediment = vec![0.0; n];
        let mut next_water = vec![0.0; n];
        let mut next_sediment = vec![0.0; n];
        let mut delta = vec![0.0; n];
        for iteration in 0..params.iterations.min(MAX_ITERATIONS) {
            next_water.iter_mut().for_each(|v| *v = 0.0);
            next_sediment.iter_mut().for_each(|v| *v = 0.0);
            delta.iter_mut().for_each(|v| *v = 0.0);
            for i in 0..n {
                let jitter = unit(mix64(params.seed ^ mix64((u64::from(iteration) << 32) | i as u64)));
                if let Some(w) = water.get_mut(i) {
                    *w += rain * (0.5 + jitter);
                }
                let (Some(&hi), Some(&w), Some(&carried)) = (self.heights.get(i), water.get(i), sediment.get(i)) else {
                    continue;
                };
                let mut lowest: Option<(usize, f64, f64)> = None;
                for &(j, run) in self.links.get(i).map(Vec::as_slice).unwrap_or_default() {
                    let Some(&hj) = self.heights.get(j) else { continue };
                    let slope = (hi - hj) / run;
                    if slope > 0.0 && lowest.is_none_or(|(_, s, _)| slope > s) {
                        lowest = Some((j, slope, hi - hj));
                    }
                }
                let slope = lowest.map_or(0.0, |l| l.1);
                let cap = capacity * w * slope;
                let (mut sed, mut dh) = (carried, 0.0);
                if w <= 1e-9 || sed > cap {
                    let share = if w <= 1e-9 { 1.0 } else { deposition };
                    let put = share * (sed - if w <= 1e-9 { 0.0 } else { cap });
                    sed -= put;
                    dh += put;
                } else {
                    let take = (strength * (cap - sed)).min(0.5 * lowest.map_or(0.0, |l| l.2));
                    sed += take;
                    dh -= take;
                }
                if let Some(d) = delta.get_mut(i) {
                    *d += dh;
                }
                let target = lowest.map_or(i, |l| l.0);
                if let Some(v) = next_water.get_mut(target) {
                    *v += w * (1.0 - evaporation);
                }
                if let Some(v) = next_sediment.get_mut(target) {
                    *v += sed;
                }
            }
            for (h, d) in self.heights.iter_mut().zip(&delta) {
                *h += d;
            }
            std::mem::swap(&mut water, &mut next_water);
            std::mem::swap(&mut sediment, &mut next_sediment);
        }
        for (h, s) in self.heights.iter_mut().zip(&sediment) {
            *h += s;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::float_cmp)]

    use super::*;

    fn hill(size: usize, peak: f64) -> HeightGraph {
        let mut heights = vec![0.0; size * size];
        let mid = size / 2;
        for y in 0..size {
            for x in 0..size {
                let d = ((x as f64 - mid as f64).powi(2) + (y as f64 - mid as f64).powi(2)).sqrt();
                heights[y * size + x] = (peak - d * peak / mid as f64 * 0.8).max(0.0);
            }
        }
        let mut links = vec![Vec::new(); size * size];
        for y in 0..size {
            for x in 0..size {
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (a, b) = (x as i64 + dx, y as i64 + dy);
                    if a >= 0 && b >= 0 && (a as usize) < size && (b as usize) < size {
                        links[y * size + x].push((b as usize * size + a as usize, 16.0));
                    }
                }
            }
        }
        HeightGraph::new(heights, links)
    }

    #[test]
    fn fractal_noise_is_deterministic_bounded_and_varies() {
        let params = NoiseParams { scale: 64.0, octaves: 4, persistence: 0.5, seed: 9 };
        let mut seen = Vec::new();
        for i in 0..200 {
            let p = DVec3::new(f64::from(i) * 13.7, f64::from(i) * 5.1 - 300.0, 40.0);
            let v = fractal_noise(p, &params);
            assert!((-1.0..=1.0).contains(&v));
            assert_eq!(v, fractal_noise(p, &params));
            seen.push(v);
        }
        let spread = seen.iter().copied().fold(f64::MIN, f64::max) - seen.iter().copied().fold(f64::MAX, f64::min);
        assert!(spread > 0.5, "{spread}");
        let other = NoiseParams { seed: 10, ..params };
        let p = DVec3::new(31.0, 17.0, 5.0);
        assert_ne!(fractal_noise(p, &params), fractal_noise(p, &other));
        let near = fractal_noise(DVec3::new(100.0, 100.0, 0.0), &params);
        let close = fractal_noise(DVec3::new(100.5, 100.0, 0.0), &params);
        assert!((near - close).abs() < 0.1, "smooth");
    }

    #[test]
    fn thermal_erosion_lowers_the_slope_and_keeps_the_mass() {
        let mut g = hill(15, 200.0);
        let (slope, total) = (g.max_slope(), g.total());
        g.thermal(&ThermalParams { iterations: 200, talus_degrees: 30.0, strength: 1.0 });
        assert!(g.max_slope() < slope * 0.5, "{} {}", g.max_slope(), slope);
        assert!((g.total() - total).abs() < 1e-6);
        let mut flat = HeightGraph::new(vec![5.0; 9], vec![Vec::new(); 9]);
        flat.thermal(&ThermalParams::default());
        assert!(flat.heights.iter().all(|h| *h == 5.0));
    }

    #[test]
    fn hydraulic_erosion_is_deterministic_conserves_mass_and_changes_the_hill() {
        let params = HydraulicParams { iterations: 80, seed: 3, ..HydraulicParams::default() };
        let (mut a, mut b) = (hill(15, 100.0), hill(15, 100.0));
        let (before, total) = (a.heights.clone(), a.total());
        a.hydraulic(&params);
        b.hydraulic(&params);
        assert_eq!(a.heights, b.heights);
        assert!((a.total() - total).abs() < 1e-6, "{} {}", a.total(), total);
        assert_ne!(a.heights, before);
        assert!(a.heights[7 * 15 + 7] < before[7 * 15 + 7], "the peak is worn down");
        let mut c = hill(15, 100.0);
        c.hydraulic(&HydraulicParams { seed: 4, ..params });
        assert_ne!(a.heights, c.heights);
        let mut dry = hill(15, 100.0);
        dry.hydraulic(&HydraulicParams { rain: 0.0, ..params });
        assert_eq!(dry.heights, before);
    }
}
