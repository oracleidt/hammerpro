use glam::DVec3;

use super::{Frame, Pick};

const INSIDE_EPS: f64 = 1e-9;
const SAME_HEIGHT: f64 = 1e-4;

#[derive(Clone, Copy, Debug)]
pub struct LocalTri {
    pub p: [DVec3; 3],
    pub up: bool,
    area2: f64,
}

impl LocalTri {
    pub fn new(world: &[DVec3; 3], frame: &Frame) -> Option<Self> {
        let n = (world[1] - world[0]).cross(world[2] - world[0]);
        let p = world.map(|w| frame.local(w));
        let area2 = (p[1].x - p[0].x) * (p[2].y - p[0].y) - (p[1].y - p[0].y) * (p[2].x - p[0].x);
        if !area2.is_finite() || area2.abs() < 1e-12 {
            return None;
        }
        Some(Self { p, up: n.dot(frame.up()) > 0.0, area2 })
    }

    pub(super) fn bounds(&self) -> (f64, f64, f64, f64) {
        let us = self.p.map(|q| q.x);
        let vs = self.p.map(|q| q.y);
        (
            us.iter().copied().fold(f64::INFINITY, f64::min),
            us.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            vs.iter().copied().fold(f64::INFINITY, f64::min),
            vs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        )
    }

    fn height_at(&self, u: f64, v: f64) -> Option<f64> {
        let [a, b, c] = self.p;
        let w0 = ((b.x - u) * (c.y - v) - (b.y - v) * (c.x - u)) / self.area2;
        let w1 = ((c.x - u) * (a.y - v) - (c.y - v) * (a.x - u)) / self.area2;
        let w2 = 1.0 - w0 - w1;
        if w0 < -INSIDE_EPS || w1 < -INSIDE_EPS || w2 < -INSIDE_EPS {
            return None;
        }
        Some(w0 * a.z + w1 * b.z + w2 * c.z)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub height: f64,
    pub overhang: bool,
}

pub struct Sampler {
    tris: Vec<LocalTri>,
    min_u: f64,
    min_v: f64,
    cell: f64,
    nu: usize,
    nv: usize,
    cells: Vec<Vec<u32>>,
    gap: f64,
}

impl Sampler {
    pub fn new(tris: Vec<LocalTri>, spacing: f64) -> Self {
        let mut min_u = f64::INFINITY;
        let mut min_v = f64::INFINITY;
        let mut max_u = f64::NEG_INFINITY;
        let mut max_v = f64::NEG_INFINITY;
        for t in &tris {
            let (a, b, c, d) = t.bounds();
            min_u = min_u.min(a);
            max_u = max_u.max(b);
            min_v = min_v.min(c);
            max_v = max_v.max(d);
        }
        if tris.is_empty() {
            return Self { tris, min_u: 0.0, min_v: 0.0, cell: 1.0, nu: 0, nv: 0, cells: Vec::new(), gap: spacing };
        }
        let ext_u = (max_u - min_u).max(spacing);
        let ext_v = (max_v - min_v).max(spacing);
        let target = (tris.len() as f64 / 2.0).clamp(1.0, 262_144.0);
        let cell = (ext_u * ext_v / target).sqrt().max(spacing * 2.0).max(ext_u.max(ext_v) / 1024.0);
        let nu = ((ext_u / cell).ceil() as usize).clamp(1, 1024);
        let nv = ((ext_v / cell).ceil() as usize).clamp(1, 1024);
        let mut cells: Vec<Vec<u32>> = vec![Vec::new(); nu * nv];
        for (k, t) in tris.iter().enumerate() {
            let (a, b, c, d) = t.bounds();
            let i0 = (((a - min_u) / cell).floor().max(0.0) as usize).min(nu - 1);
            let i1 = (((b - min_u) / cell).floor().max(0.0) as usize).min(nu - 1);
            let j0 = (((c - min_v) / cell).floor().max(0.0) as usize).min(nv - 1);
            let j1 = (((d - min_v) / cell).floor().max(0.0) as usize).min(nv - 1);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    cells[j * nu + i].push(k as u32);
                }
            }
        }
        Self { tris, min_u, min_v, cell, nu, nv, cells, gap: (spacing * 2.0).max(2.0) }
    }

    pub fn sample(&self, u: f64, v: f64, pick: Pick, scratch: &mut Vec<(f64, bool)>) -> Option<Sample> {
        if self.nu == 0 {
            return None;
        }
        let fi = (u - self.min_u) / self.cell;
        let fj = (v - self.min_v) / self.cell;
        if fi < -1e-6 || fj < -1e-6 || fi > self.nu as f64 + 1e-6 || fj > self.nv as f64 + 1e-6 {
            return None;
        }
        let i = (fi.max(0.0) as usize).min(self.nu - 1);
        let j = (fj.max(0.0) as usize).min(self.nv - 1);
        scratch.clear();
        for &k in &self.cells[j * self.nu + i] {
            let t = &self.tris[k as usize];
            if let Some(h) = t.height_at(u, v) {
                scratch.push((h, t.up));
            }
        }
        if scratch.is_empty() {
            return None;
        }
        let mut best: Option<f64> = None;
        for &(h, up) in scratch.iter() {
            if (pick == Pick::Top || up) && best.is_none_or(|b| h > b) {
                best = Some(h);
            }
        }
        let height = best?;
        let mut layers: Vec<f64> = scratch.iter().filter(|(_, up)| *up).map(|(h, _)| *h).collect();
        layers.sort_by(|a, b| b.total_cmp(a));
        let mut distinct = 0usize;
        let mut last = f64::INFINITY;
        let mut first = 0.0;
        for h in layers {
            if (last - h).abs() > SAME_HEIGHT {
                if distinct == 0 {
                    first = h;
                }
                distinct += 1;
                last = h;
            }
        }
        let overhang = distinct >= 2 && (first - last) > self.gap;
        Some(Sample { height, overhang })
    }
}
