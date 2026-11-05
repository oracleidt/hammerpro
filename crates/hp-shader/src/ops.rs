use rayon::prelude::*;

pub type V4 = [f32; 4];

pub fn lum(c: V4) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

pub fn fv(v: f32) -> V4 {
    [v, v, v, 1.0]
}

pub fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    if (e1 - e0).abs() < 1e-6 {
        return if x >= e1 { 1.0 } else { 0.0 };
    }
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn lerp4(a: V4, b: V4, t: f32) -> V4 {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t, a[3] + (b[3] - a[3]) * t]
}

pub fn rgb_to_hsv(c: V4) -> [f32; 3] {
    let (r, g, b) = (c[0], c[1], c[2]);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= 1e-6 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / d + 2.0) / 6.0
    } else {
        ((r - g) / d + 4.0) / 6.0
    };
    let s = if max <= 1e-6 { 0.0 } else { d / max };
    [h, s, max]
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor();
    let f = h - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    match i as i32 {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

pub fn blend_channel(mode: &str, b: f32, l: f32) -> f32 {
    match mode {
        "multiply" => b * l,
        "screen" => 1.0 - (1.0 - b) * (1.0 - l),
        "overlay" => {
            if b < 0.5 {
                2.0 * b * l
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - l)
            }
        }
        "hard_light" => {
            if l < 0.5 {
                2.0 * b * l
            } else {
                1.0 - 2.0 * (1.0 - b) * (1.0 - l)
            }
        }
        "soft_light" => {
            if l <= 0.5 {
                b - (1.0 - 2.0 * l) * b * (1.0 - b)
            } else {
                let d = if b <= 0.25 { ((16.0 * b - 12.0) * b + 4.0) * b } else { b.sqrt() };
                b + (2.0 * l - 1.0) * (d - b)
            }
        }
        "add" => b + l,
        "subtract" => b - l,
        "difference" => (b - l).abs(),
        "darken" => b.min(l),
        "lighten" => b.max(l),
        "color_dodge" => {
            if l >= 1.0 {
                1.0
            } else {
                (b / (1.0 - l)).min(1.0)
            }
        }
        "color_burn" => {
            if l <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / l).min(1.0)
            }
        }
        "exclusion" => b + l - 2.0 * b * l,
        _ => l,
    }
}

fn hash(x: i32, y: i32, seed: u32) -> u32 {
    let mut h =
        (x as u32).wrapping_mul(0x27d4_eb2d) ^ (y as u32).wrapping_mul(0x1656_67b1) ^ seed.wrapping_mul(0x9e37_79b1);
    h ^= h >> 15;
    h = h.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 13;
    h = h.wrapping_mul(0xc2b2_ae35);
    h ^ (h >> 16)
}

fn unit(h: u32) -> f32 {
    (h >> 8) as f32 / 16_777_216.0
}

fn grad(h: u32, dx: f32, dy: f32) -> f32 {
    let a = unit(h) * std::f32::consts::TAU;
    a.cos() * dx + a.sin() * dy
}

fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn perlin(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (ix, iy) = (x0 as i32, y0 as i32);
    let at = |dx: i32, dy: i32| hash((ix + dx).rem_euclid(period), (iy + dy).rem_euclid(period), seed);
    let n00 = grad(at(0, 0), fx, fy);
    let n10 = grad(at(1, 0), fx - 1.0, fy);
    let n01 = grad(at(0, 1), fx, fy - 1.0);
    let n11 = grad(at(1, 1), fx - 1.0, fy - 1.0);
    let (u, v) = (fade(fx), fade(fy));
    let a = n00 + (n10 - n00) * u;
    let b = n01 + (n11 - n01) * u;
    ((a + (b - a) * v) * 1.4).clamp(-1.0, 1.0)
}

fn simplex(x: f32, y: f32, seed: u32) -> f32 {
    const F2: f32 = 0.366_025_4;
    const G2: f32 = 0.211_324_87;
    let s = (x + y) * F2;
    let (i, j) = ((x + s).floor(), (y + s).floor());
    let t = (i + j) * G2;
    let (x0, y0) = (x - (i - t), y - (j - t));
    let (i1, j1) = if x0 > y0 { (1, 0) } else { (0, 1) };
    let (x1, y1) = (x0 - i1 as f32 + G2, y0 - j1 as f32 + G2);
    let (x2, y2) = (x0 - 1.0 + 2.0 * G2, y0 - 1.0 + 2.0 * G2);
    let (ii, jj) = (i as i32, j as i32);
    let corner = |dx: f32, dy: f32, ox: i32, oy: i32| {
        let t = 0.5 - dx * dx - dy * dy;
        if t < 0.0 {
            0.0
        } else {
            let t2 = t * t;
            t2 * t2 * grad(hash(ii + ox, jj + oy, seed), dx, dy)
        }
    };
    (70.0 * (corner(x0, y0, 0, 0) + corner(x1, y1, i1, j1) + corner(x2, y2, 1, 1))).clamp(-1.0, 1.0)
}

fn voronoi(x: f32, y: f32, period: i32, seed: u32, jitter: f32) -> (f32, f32, f32) {
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    let (mut f1, mut f2, mut id) = (f32::MAX, f32::MAX, 0.0);
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (nx, ny) = (cx + dx, cy + dy);
            let (wx, wy) = (nx.rem_euclid(period), ny.rem_euclid(period));
            let px = nx as f32 + 0.5 + (unit(hash(wx, wy, seed)) - 0.5) * jitter;
            let py = ny as f32 + 0.5 + (unit(hash(wx, wy, seed ^ 0x55aa_55aa)) - 0.5) * jitter;
            let d = ((px - x) * (px - x) + (py - y) * (py - y)).sqrt();
            if d < f1 {
                f2 = f1;
                f1 = d;
                id = unit(hash(wx, wy, seed ^ 0x1234_5678));
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    (f1, f2, id)
}

pub struct NoiseSpec<'a> {
    pub kind: &'a str,
    pub voronoi: &'a str,
    pub scale: i32,
    pub octaves: i32,
    pub lacunarity: i32,
    pub gain: f32,
    pub seed: u32,
    pub jitter: f32,
}

fn single(spec: &NoiseSpec<'_>, u: f32, v: f32, period: i32, seed: u32) -> f32 {
    let (fu, fv) = (u.rem_euclid(1.0), v.rem_euclid(1.0));
    let p = period as f32;
    let (x, y) = (fu * p, fv * p);
    match spec.kind {
        "simplex" => {
            let n = |x: f32, y: f32| simplex(x, y, seed);
            let t = n(x, y) * (1.0 - fu) * (1.0 - fv)
                + n(x - p, y) * fu * (1.0 - fv)
                + n(x - p, y - p) * fu * fv
                + n(x, y - p) * (1.0 - fu) * fv;
            0.5 + 0.5 * t
        }
        "voronoi" => {
            let (f1, f2, id) = voronoi(x, y, period, seed, spec.jitter);
            match spec.voronoi {
                "edges" => 1.0 - smooth(0.0, 0.2, f2 - f1),
                "cells" => id,
                _ => f1.clamp(0.0, 1.0),
            }
        }
        "white" => unit(hash((x.floor() as i32).rem_euclid(period), (y.floor() as i32).rem_euclid(period), seed)),
        _ => 0.5 + 0.5 * perlin(x, y, period, seed),
    }
}

pub fn noise(spec: &NoiseSpec<'_>, u: f32, v: f32) -> f32 {
    let (mut sum, mut norm, mut amp) = (0.0, 0.0, 1.0);
    let mut period = spec.scale.max(1);
    for o in 0..spec.octaves.max(1) {
        sum += amp * single(spec, u, v, period, spec.seed.wrapping_add(o as u32 * 7919));
        norm += amp;
        amp *= spec.gain;
        period = period.saturating_mul(spec.lacunarity.max(2)).min(4096);
    }
    if norm <= 0.0 { 0.0 } else { (sum / norm).clamp(0.0, 1.0) }
}

pub fn ramp(stops: &[(f32, V4)], t: f32, constant: bool) -> V4 {
    let Some(first) = stops.first() else { return fv(t) };
    if t <= first.0 {
        return first.1;
    }
    for pair in stops.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if t <= b.0 {
            if constant {
                return a.1;
            }
            let span = b.0 - a.0;
            let k = if span <= 1e-6 { 1.0 } else { (t - a.0) / span };
            return lerp4(a.1, b.1, k);
        }
    }
    stops.last().map(|s| s.1).unwrap_or([0.0; 4])
}

fn transpose(src: &[V4], w: usize, h: usize) -> Vec<V4> {
    let mut out = vec![[0.0; 4]; src.len()];
    out.par_chunks_mut(h).enumerate().for_each(|(x, col)| {
        for (y, p) in col.iter_mut().enumerate() {
            *p = src[y * w + x];
        }
    });
    out
}

fn blur_rows(src: &[V4], w: usize, r: usize) -> Vec<V4> {
    let mut out = vec![[0.0; 4]; src.len()];
    let inv = 1.0 / (2 * r + 1) as f32;
    out.par_chunks_mut(w).zip(src.par_chunks(w)).for_each(|(dst, row)| {
        let mut sum = [0.0f32; 4];
        for k in 0..=2 * r {
            let i = (k + w * 2 - r) % w;
            for c in 0..4 {
                sum[c] += row[i][c];
            }
        }
        for x in 0..w {
            dst[x] = [sum[0] * inv, sum[1] * inv, sum[2] * inv, sum[3] * inv];
            let add = row[(x + r + 1) % w];
            let sub = row[(x + w * 2 - r) % w];
            for c in 0..4 {
                sum[c] += add[c] - sub[c];
            }
        }
    });
    out
}

pub fn gaussian(src: &[V4], w: usize, h: usize, radius: f32) -> Vec<V4> {
    let limit = (w.min(h).saturating_sub(1)) / 2;
    let r = (radius.round().max(0.0) as usize).min(limit);
    if r == 0 || w < 3 || h < 3 {
        return src.to_vec();
    }
    let mut cur = src.to_vec();
    for _ in 0..3 {
        cur = blur_rows(&cur, w, r);
        let t = transpose(&cur, w, h);
        let t = blur_rows(&t, h, r);
        cur = transpose(&t, h, w);
    }
    cur
}

pub fn wrap_index(v: i64, n: usize) -> usize {
    v.rem_euclid(n as i64) as usize
}

pub fn sobel(vals: &[f32], w: usize, h: usize, x: usize, y: usize, d: i64) -> (f32, f32) {
    let at = |dx: i64, dy: i64| vals[wrap_index(y as i64 + dy, h) * w + wrap_index(x as i64 + dx, w)];
    let gx = (at(d, -d) + 2.0 * at(d, 0) + at(d, d)) - (at(-d, -d) + 2.0 * at(-d, 0) + at(-d, d));
    let gy = (at(-d, d) + 2.0 * at(0, d) + at(d, d)) - (at(-d, -d) + 2.0 * at(0, -d) + at(d, -d));
    (gx, gy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trip() {
        for c in [[0.2, 0.4, 0.8, 1.0], [1.0, 0.0, 0.0, 1.0], [0.3, 0.3, 0.3, 1.0], [0.9, 0.7, 0.1, 1.0]] {
            let [h, s, v] = rgb_to_hsv(c);
            let back = hsv_to_rgb(h, s, v);
            for i in 0..3 {
                assert!((back[i] - c[i]).abs() < 1e-4, "{c:?} {back:?}");
            }
        }
    }

    #[test]
    fn noise_tiles_and_stays_in_range() {
        for kind in ["perlin", "simplex", "voronoi", "white"] {
            let spec = NoiseSpec {
                kind,
                voronoi: "distance",
                scale: 6,
                octaves: 3,
                lacunarity: 2,
                gain: 0.5,
                seed: 3,
                jitter: 1.0,
            };
            for i in 0..40 {
                let (u, v) = (i as f32 * 0.0231, i as f32 * 0.0177);
                let a = noise(&spec, u, v);
                assert!((0.0..=1.0).contains(&a), "{kind} {a}");
                let b = noise(&spec, u + 1.0, v + 2.0);
                assert!((a - b).abs() < 1e-3, "{kind} must repeat: {a} {b}");
            }
        }
        let spec = NoiseSpec {
            kind: "perlin",
            voronoi: "distance",
            scale: 8,
            octaves: 1,
            lacunarity: 2,
            gain: 0.5,
            seed: 1,
            jitter: 1.0,
        };
        let near = (noise(&spec, 0.0001, 0.3) - noise(&spec, 0.9999, 0.3)).abs();
        assert!(near < 0.01, "no seam at the edge: {near}");
    }

    #[test]
    fn blur_keeps_flat_images_and_mean() {
        let flat = vec![[0.25, 0.5, 0.75, 1.0]; 16 * 16];
        let b = gaussian(&flat, 16, 16, 3.0);
        for p in &b {
            for c in 0..4 {
                assert!((p[c] - flat[0][c]).abs() < 1e-4);
            }
        }
        let mut img = vec![[0.0, 0.0, 0.0, 1.0]; 16 * 16];
        img[8 * 16 + 8] = [1.0, 1.0, 1.0, 1.0];
        let b = gaussian(&img, 16, 16, 2.0);
        let total: f32 = b.iter().map(|p| p[0]).sum();
        assert!((total - 1.0).abs() < 1e-3);
        assert!(b[8 * 16 + 8][0] < 1.0);
        assert!(b[8 * 16 + 9][0] > 0.0);
    }

    #[test]
    fn ramp_interpolates() {
        let stops = [(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 1.0, 1.0, 1.0])];
        assert_eq!(ramp(&stops, 0.5, false)[0], 0.5);
        assert_eq!(ramp(&stops, 0.5, true)[0], 0.0);
        assert_eq!(ramp(&stops, 2.0, false)[0], 1.0);
        assert_eq!(ramp(&[], 0.3, false)[0], 0.3);
    }

    #[test]
    fn blend_modes_match_definitions() {
        assert_eq!(blend_channel("multiply", 0.5, 0.5), 0.25);
        assert_eq!(blend_channel("screen", 0.5, 0.5), 0.75);
        assert_eq!(blend_channel("overlay", 0.25, 0.5), 0.25);
        assert_eq!(blend_channel("normal", 0.1, 0.9), 0.9);
        assert!((blend_channel("soft_light", 0.5, 0.5) - 0.5).abs() < 1e-6);
        assert_eq!(blend_channel("difference", 0.2, 0.7), 0.5);
    }
}
