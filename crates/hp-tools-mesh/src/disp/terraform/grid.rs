use glam::DVec3;
use hp_editor::{Document, FaceRef};

pub const MAX_CELLS: usize = 112;
const MAX_DROPLET_STEPS: usize = 48;
const MAX_SPEED: f64 = 3.0;
const MAX_CAPACITY: f64 = 0.6;
pub const MAX_STEP_CELLS: f64 = 0.25;

#[derive(Clone, Debug)]
pub struct Grid {
    pub origin: DVec3,
    pub t1: DVec3,
    pub t2: DVec3,
    pub up: DVec3,
    pub cell: f64,
    pub half: f64,
    pub n: usize,
    pub h: Vec<f64>,
}

pub fn basis(up: DVec3) -> (DVec3, DVec3) {
    crate::common::tangent_basis(up)
}

pub fn spacing(doc: &Document, faces: &[FaceRef]) -> Option<f64> {
    let mut best: Option<f64> = None;
    for f in faces {
        let Some(mesh) = doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)) else { continue };
        if mesh.row_len < 2 {
            continue;
        }
        let (Some(a), Some(b), Some(c)) =
            (mesh.base_positions.first(), mesh.base_positions.get(1), mesh.base_positions.get(mesh.row_len))
        else {
            continue;
        };
        let s = a.distance(*b).min(a.distance(*c));
        if s.is_finite() && s > 1e-6 {
            best = Some(best.map_or(s, |x: f64| x.min(s)));
        }
    }
    best
}

impl Grid {
    pub fn build(doc: &Document, faces: &[FaceRef], center: DVec3, up: DVec3, half: f64) -> Option<Grid> {
        Self::build_with(doc, faces, center, up, half, 0.85)
    }

    pub fn build_with(
        doc: &Document,
        faces: &[FaceRef],
        center: DVec3,
        up: DVec3,
        half: f64,
        density: f64,
    ) -> Option<Grid> {
        let up = up.try_normalize()?;
        if !center.is_finite() || !half.is_finite() || half <= 0.0 {
            return None;
        }
        let (t1, t2) = basis(up);
        let space = spacing(doc, faces).unwrap_or(half / 16.0);
        let cell = (space * density).max(2.0 * half / MAX_CELLS as f64).max(1e-3);
        let n = ((2.0 * half / cell).ceil() as usize + 1).clamp(4, MAX_CELLS + 1);
        let cell = 2.0 * half / (n - 1) as f64;
        let origin = center - up * up.dot(center);
        let mut grid = Grid { origin, t1, t2, up, cell, half, n, h: vec![0.0; n * n] };
        let mut acc = vec![f64::NEG_INFINITY; n * n];
        let last = (n - 1) as f64;
        let mut pts: Vec<(f64, f64, f64)> = Vec::new();
        for f in faces {
            let Some(mesh) = doc.scene().solid(f.solid).and_then(|node| node.disp_mesh(f.side)) else { continue };
            pts.clear();
            pts.extend(mesh.positions.iter().map(|p| {
                let (x, y) = grid.xy(*p);
                (x, y, up.dot(*p))
            }));
            for t in mesh.indices.chunks_exact(3) {
                let (Some(a), Some(b), Some(c)) =
                    (pts.get(t[0] as usize), pts.get(t[1] as usize), pts.get(t[2] as usize))
                else {
                    continue;
                };
                let (x0, x1) = (a.0.min(b.0).min(c.0), a.0.max(b.0).max(c.0));
                let (y0, y1) = (a.1.min(b.1).min(c.1), a.1.max(b.1).max(c.1));
                if x1 < 0.0 || y1 < 0.0 || x0 > last || y0 > last {
                    continue;
                }
                let denom = (b.1 - c.1) * (a.0 - c.0) + (c.0 - b.0) * (a.1 - c.1);
                if denom.abs() < 1e-12 {
                    continue;
                }
                let (cx0, cx1) = (x0.max(0.0).ceil() as usize, x1.min(last).floor() as usize);
                let (cy0, cy1) = (y0.max(0.0).ceil() as usize, y1.min(last).floor() as usize);
                for cy in cy0..=cy1 {
                    for cx in cx0..=cx1 {
                        let (px, py) = (cx as f64, cy as f64);
                        let w1 = ((b.1 - c.1) * (px - c.0) + (c.0 - b.0) * (py - c.1)) / denom;
                        let w2 = ((c.1 - a.1) * (px - c.0) + (a.0 - c.0) * (py - c.1)) / denom;
                        let w3 = 1.0 - w1 - w2;
                        if w1 < -1e-9 || w2 < -1e-9 || w3 < -1e-9 {
                            continue;
                        }
                        let k = cy * n + cx;
                        acc[k] = acc[k].max(w1 * a.2 + w2 * b.2 + w3 * c.2);
                    }
                }
            }
        }
        let wsum: Vec<f64> = acc.iter().map(|v| if v.is_finite() { 1.0 } else { 0.0 }).collect();
        for v in &mut acc {
            if !v.is_finite() {
                *v = 0.0;
            }
        }
        let mut known: Vec<bool> = wsum.iter().map(|w| *w > 1e-6).collect();
        if !known.iter().any(|k| *k) {
            return None;
        }
        for k in 0..n * n {
            if known[k] {
                grid.h[k] = acc[k] / wsum[k];
            }
        }
        for _ in 0..n {
            let mut changed = false;
            let mut next = grid.h.clone();
            let mut next_known = known.clone();
            for y in 0..n {
                for x in 0..n {
                    let k = y * n + x;
                    if known[k] {
                        continue;
                    }
                    let mut s = 0.0;
                    let mut c = 0.0;
                    for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                        let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                        if nx < 0 || ny < 0 || nx >= n as i64 || ny >= n as i64 {
                            continue;
                        }
                        let j = ny as usize * n + nx as usize;
                        if known[j] {
                            s += grid.h[j];
                            c += 1.0;
                        }
                    }
                    if c > 0.0 {
                        next[k] = s / c;
                        next_known[k] = true;
                        changed = true;
                    }
                }
            }
            grid.h = next;
            known = next_known;
            if !changed {
                break;
            }
        }
        Some(grid)
    }

    pub fn xy(&self, p: DVec3) -> (f64, f64) {
        let d = p - self.origin;
        ((d.dot(self.t1) + self.half) / self.cell, (d.dot(self.t2) + self.half) / self.cell)
    }

    pub fn center_xy(&self) -> f64 {
        self.half / self.cell
    }

    pub fn sample(&self, data: &[f64], x: f64, y: f64) -> Option<f64> {
        let max = (self.n - 1) as f64;
        if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 || x > max || y > max {
            return None;
        }
        let (x0, y0) = ((x.floor() as usize).min(self.n - 2), (y.floor() as usize).min(self.n - 2));
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let at = |cx: usize, cy: usize| data.get(cy * self.n + cx).copied().unwrap_or(0.0);
        let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * fx;
        let bottom = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * fx;
        Some(top + (bottom - top) * fy)
    }

    pub fn height_at(&self, p: DVec3) -> Option<f64> {
        let (x, y) = self.xy(p);
        self.sample(&self.h, x, y)
    }
}

pub fn blur(data: &[f64], n: usize, radius: usize) -> Vec<f64> {
    let r = radius as i64;
    let mut tmp = vec![0.0; data.len()];
    let mut out = vec![0.0; data.len()];
    for y in 0..n {
        for x in 0..n {
            let mut s = 0.0;
            let mut c = 0.0;
            for d in -r..=r {
                let xx = x as i64 + d;
                if xx >= 0 && xx < n as i64 {
                    s += data[y * n + xx as usize];
                    c += 1.0;
                }
            }
            tmp[y * n + x] = s / c;
        }
    }
    for y in 0..n {
        for x in 0..n {
            let mut s = 0.0;
            let mut c = 0.0;
            for d in -r..=r {
                let yy = y as i64 + d;
                if yy >= 0 && yy < n as i64 {
                    s += tmp[yy as usize * n + x];
                    c += 1.0;
                }
            }
            out[y * n + x] = s / c;
        }
    }
    out
}

pub fn local_range(data: &[f64], n: usize, radius: usize) -> (Vec<f64>, Vec<f64>) {
    let r = radius as i64;
    let mut lo = vec![0.0; data.len()];
    let mut hi = vec![0.0; data.len()];
    for y in 0..n {
        for x in 0..n {
            let (mut a, mut b) = (f64::INFINITY, f64::NEG_INFINITY);
            for dy in -r..=r {
                let yy = y as i64 + dy;
                if yy < 0 || yy >= n as i64 {
                    continue;
                }
                for dx in -r..=r {
                    let xx = x as i64 + dx;
                    if xx < 0 || xx >= n as i64 {
                        continue;
                    }
                    let v = data[yy as usize * n + xx as usize];
                    a = a.min(v);
                    b = b.max(v);
                }
            }
            lo[y * n + x] = a;
            hi[y * n + x] = b;
        }
    }
    (lo, hi)
}

pub fn thermal(h: &mut [f64], n: usize, cell: f64, talus_deg: f64, strength: f64, passes: usize) {
    let talus = talus_deg.to_radians().tan() * cell;
    let diag = std::f64::consts::SQRT_2;
    let dirs = [
        (-1i64, 0i64, 1.0),
        (1, 0, 1.0),
        (0, -1, 1.0),
        (0, 1, 1.0),
        (-1, -1, diag),
        (1, -1, diag),
        (-1, 1, diag),
        (1, 1, diag),
    ];
    let mut delta = vec![0.0; h.len()];
    for _ in 0..passes {
        delta.iter_mut().for_each(|d| *d = 0.0);
        for y in 1..n.saturating_sub(1) {
            for x in 1..n.saturating_sub(1) {
                let k = y * n + x;
                let mut excess = [0.0; 8];
                let mut sum = 0.0;
                let mut most: f64 = 0.0;
                for (i, (dx, dy, dist)) in dirs.iter().enumerate() {
                    let j = (y as i64 + dy) as usize * n + (x as i64 + dx) as usize;
                    let e = h[k] - h[j] - talus * dist;
                    if e > 0.0 {
                        excess[i] = e;
                        sum += e;
                        most = most.max(e);
                    }
                }
                if sum <= 0.0 {
                    continue;
                }
                let moved = most * 0.5 * strength;
                delta[k] -= moved;
                for (i, (dx, dy, _)) in dirs.iter().enumerate() {
                    if excess[i] > 0.0 {
                        let j = (y as i64 + dy) as usize * n + (x as i64 + dx) as usize;
                        delta[j] += moved * excess[i] / sum;
                    }
                }
            }
        }
        for (v, d) in h.iter_mut().zip(&delta) {
            *v += d;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct DropletParams {
    pub inertia: f64,
    pub capacity: f64,
    pub deposition: f64,
    pub erosion: f64,
    pub evaporation: f64,
    pub radius: f64,
}

pub fn next_random(state: &mut u64) -> f64 {
    let mut x = *state;
    if x == 0 {
        x = 0x9E37_79B9_7F4A_7C15;
    }
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

fn height_grad(h: &[f64], n: usize, x: f64, y: f64) -> (f64, f64, f64) {
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let at = |cx: usize, cy: usize| h[cy * n + cx];
    let (a, b, c, d) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
    let gx = (b - a) * (1.0 - fy) + (d - c) * fy;
    let gy = (c - a) * (1.0 - fx) + (d - b) * fx;
    let hgt = a * (1.0 - fx) * (1.0 - fy) + b * fx * (1.0 - fy) + c * (1.0 - fx) * fy + d * fx * fy;
    (hgt, gx, gy)
}

pub fn hydraulic(h: &mut [f64], n: usize, droplets: usize, p: &DropletParams, rng: &mut u64) {
    if n < 4 {
        return;
    }
    let c = (n - 1) as f64 * 0.5;
    let max = (n - 1) as f64 - 1e-6;
    let radius = p.radius.min(c);
    for _ in 0..droplets {
        let a = next_random(rng) * std::f64::consts::TAU;
        let r = radius * next_random(rng).sqrt() * 0.95;
        let (mut x, mut y) = (c + r * a.cos(), c + r * a.sin());
        let (mut dx, mut dy) = (0.0, 0.0);
        let (mut speed, mut water, mut sediment) = (1.0f64, 1.0f64, 0.0f64);
        for _ in 0..MAX_DROPLET_STEPS {
            if x < 0.0 || y < 0.0 || x >= max || y >= max {
                break;
            }
            let (hgt, gx, gy) = height_grad(h, n, x, y);
            dx = dx * p.inertia - gx * (1.0 - p.inertia);
            dy = dy * p.inertia - gy * (1.0 - p.inertia);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-9 {
                let t = next_random(rng) * std::f64::consts::TAU;
                (dx, dy) = (t.cos(), t.sin());
            } else {
                dx /= len;
                dy /= len;
            }
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0.0 || ny < 0.0 || nx >= max || ny >= max || ((nx - c).powi(2) + (ny - c).powi(2)).sqrt() > c {
                break;
            }
            let (nh, _, _) = height_grad(h, n, nx, ny);
            let dh = nh - hgt;
            let cap = ((-dh).max(0.01) * speed * water * p.capacity).min(MAX_CAPACITY);
            if sediment > cap || dh > 0.0 {
                let amount = if dh > 0.0 { dh.min(sediment) } else { (sediment - cap) * p.deposition };
                sediment -= amount;
                deposit(h, n, x, y, amount);
            } else {
                let amount = ((cap - sediment) * p.erosion).min(-dh);
                erode(h, n, x, y, amount);
                sediment += amount;
            }
            speed = (speed * speed - dh * 4.0).max(0.0).sqrt().min(MAX_SPEED);
            water *= 1.0 - p.evaporation;
            (x, y) = (nx, ny);
        }
    }
}

fn deposit(h: &mut [f64], n: usize, x: f64, y: f64, amount: f64) {
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    for (cx, cy, w) in [
        (x0, y0, (1.0 - fx) * (1.0 - fy)),
        (x0 + 1, y0, fx * (1.0 - fy)),
        (x0, y0 + 1, (1.0 - fx) * fy),
        (x0 + 1, y0 + 1, fx * fy),
    ] {
        if let Some(v) = h.get_mut(cy * n + cx) {
            *v += amount * w;
        }
    }
}

fn erode(h: &mut [f64], n: usize, x: f64, y: f64, amount: f64) {
    let r = 2.6f64;
    let (cx, cy) = (x.round() as i64, y.round() as i64);
    let mut cells: [(usize, f64); 49] = [(0, 0.0); 49];
    let mut count = 0;
    let mut total = 0.0;
    for dy in -3..=3i64 {
        for dx in -3..=3i64 {
            let (px, py) = (cx + dx, cy + dy);
            if px < 0 || py < 0 || px >= n as i64 || py >= n as i64 {
                continue;
            }
            let d = ((px as f64 - x).powi(2) + (py as f64 - y).powi(2)).sqrt();
            let w = r - d;
            if w > 0.0 {
                cells[count] = (py as usize * n + px as usize, w);
                count += 1;
                total += w;
            }
        }
    }
    if total <= 0.0 {
        return;
    }
    for (k, w) in &cells[..count] {
        if let Some(v) = h.get_mut(*k) {
            *v -= amount * w / total;
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn thermal_lowers_a_spike_and_keeps_mass() {
        let n = 9;
        let mut h = vec![0.0; n * n];
        h[4 * n + 4] = 50.0;
        let before: f64 = h.iter().sum();
        thermal(&mut h, n, 1.0, 30.0, 0.5, 20);
        assert!(h[4 * n + 4] < 25.0);
        assert!((h.iter().sum::<f64>() - before).abs() < 1e-6);
    }

    #[test]
    fn droplets_carve_a_slope() {
        let n = 33;
        let mut h: Vec<f64> = (0..n * n).map(|k| (k % n) as f64 * 0.6).collect();
        let before = h.clone();
        let p = DropletParams {
            inertia: 0.05,
            capacity: 4.0,
            deposition: 0.3,
            erosion: 0.3,
            evaporation: 0.02,
            radius: 16.0,
        };
        let mut rng = 7;
        hydraulic(&mut h, n, 400, &p, &mut rng);
        let dug = h.iter().zip(&before).filter(|(a, b)| *a < *b).count();
        assert!(dug > 20);
    }

    #[test]
    fn blur_keeps_a_flat_field() {
        let h = vec![3.0; 25];
        assert!(blur(&h, 5, 2).iter().all(|v| (v - 3.0).abs() < 1e-12));
    }
}
