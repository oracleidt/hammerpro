use std::collections::HashMap;

use glam::{DVec2, DVec3};

pub const BLEND_CHANNELS: usize = 4;
pub const MIN_BLEND_WIDTH: f64 = 0.01;
pub const MAX_BLEND_WIDTH: f64 = 65536.0;
const TOUCH: f64 = 1e-3;
const TIE: f64 = 1e-6;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendFalloff {
    Linear,
    #[default]
    Smooth,
    EaseIn,
    EaseOut,
}

impl BlendFalloff {
    pub const ALL: [BlendFalloff; 4] =
        [BlendFalloff::Linear, BlendFalloff::Smooth, BlendFalloff::EaseIn, BlendFalloff::EaseOut];

    pub fn eval(self, u: f64) -> f64 {
        let u = if u.is_finite() { u.clamp(0.0, 1.0) } else { 0.0 };
        match self {
            BlendFalloff::Linear => u,
            BlendFalloff::Smooth => u * u * (3.0 - 2.0 * u),
            BlendFalloff::EaseIn => u * u,
            BlendFalloff::EaseOut => 1.0 - (1.0 - u) * (1.0 - u),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    #[default]
    Average,
    AOverB,
    BOverA,
    Max,
    Min,
}

impl BlendMode {
    pub const ALL: [BlendMode; 5] =
        [BlendMode::Average, BlendMode::AOverB, BlendMode::BOverA, BlendMode::Max, BlendMode::Min];

    fn accepts(self, own: usize, other: usize) -> bool {
        match self {
            BlendMode::AOverB => other < own,
            BlendMode::BOverA => other > own,
            _ => other != own,
        }
    }

    fn nominal(self) -> f64 {
        match self {
            BlendMode::AOverB | BlendMode::BOverA => 1.0,
            _ => 0.5,
        }
    }

    fn target(self, own: f64, other: f64) -> f64 {
        match self {
            BlendMode::Average => 0.5 * (own + other),
            BlendMode::AOverB | BlendMode::BOverA => other,
            BlendMode::Max => own.max(other),
            BlendMode::Min => own.min(other),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendParams {
    pub width: f64,
    pub falloff: BlendFalloff,
    pub mode: BlendMode,
    pub match_edges: bool,
    pub alpha: bool,
}

impl Default for BlendParams {
    fn default() -> Self {
        Self { width: 128.0, falloff: BlendFalloff::Smooth, mode: BlendMode::Average, match_edges: false, alpha: true }
    }
}

#[derive(Clone, Debug, Default)]
pub struct BlendPart {
    pub set: usize,
    pub points: Vec<DVec3>,
    pub values: Vec<[f64; BLEND_CHANNELS]>,
    pub tris: Vec<[u32; 3]>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlendResult {
    pub heights: Vec<f64>,
    pub values: Vec<[f64; BLEND_CHANNELS]>,
    pub influence: Vec<f64>,
}

struct Hit {
    dist: f64,
    z: f64,
    value: [f64; BLEND_CHANNELS],
}

struct TriIndex<'a> {
    parts: &'a [BlendPart],
    cell: f64,
    cells: HashMap<(i64, i64), Vec<(u32, u32)>>,
}

fn closest_on_triangle(p: DVec2, a: DVec3, b: DVec3, c: DVec3) -> (f64, [f64; 3]) {
    let (pa, pb, pc) = (a.truncate(), b.truncate(), c.truncate());
    let (e1, e2, q) = (pb - pa, pc - pa, p - pa);
    let det = e1.x * e2.y - e1.y * e2.x;
    if det.abs() > 1e-12 {
        let l1 = (q.x * e2.y - q.y * e2.x) / det;
        let l2 = (e1.x * q.y - e1.y * q.x) / det;
        let l0 = 1.0 - l1 - l2;
        if l0 >= -1e-9 && l1 >= -1e-9 && l2 >= -1e-9 {
            return (0.0, [l0, l1, l2]);
        }
    }
    let corners = [pa, pb, pc];
    let mut best = (f64::INFINITY, [1.0, 0.0, 0.0]);
    for i in 0..3 {
        let j = (i + 1) % 3;
        let (u, v) = (corners[i], corners[j]);
        let d = v - u;
        let len2 = d.length_squared();
        let t = if len2 > 1e-18 { ((p - u).dot(d) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let dist = (u + d * t - p).length();
        if dist < best.0 {
            let mut bary = [0.0; 3];
            bary[i] = 1.0 - t;
            bary[j] = t;
            best = (dist, bary);
        }
    }
    best
}

impl<'a> TriIndex<'a> {
    fn new(parts: &'a [BlendPart], reach: f64) -> Self {
        let mut extent = 0.0;
        let mut count = 0usize;
        for part in parts {
            for t in &part.tris {
                let pts = t.map(|i| part.points.get(i as usize).copied().unwrap_or(DVec3::ZERO));
                let lo = pts[0].min(pts[1]).min(pts[2]);
                let hi = pts[0].max(pts[1]).max(pts[2]);
                extent += (hi.x - lo.x).max(hi.y - lo.y);
                count += 1;
            }
        }
        let average = if count > 0 { extent / count as f64 } else { 64.0 };
        let cell = (average * 2.0).max(reach / 8.0).max(1.0);
        let mut cells: HashMap<(i64, i64), Vec<(u32, u32)>> = HashMap::new();
        for (pi, part) in parts.iter().enumerate() {
            for (ti, t) in part.tris.iter().enumerate() {
                let pts = t.map(|i| part.points.get(i as usize).copied().unwrap_or(DVec3::ZERO));
                if pts.iter().any(|p| !p.is_finite()) {
                    continue;
                }
                let lo = pts[0].min(pts[1]).min(pts[2]);
                let hi = pts[0].max(pts[1]).max(pts[2]);
                let (x0, x1) = ((lo.x / cell).floor() as i64, (hi.x / cell).floor() as i64);
                let (y0, y1) = ((lo.y / cell).floor() as i64, (hi.y / cell).floor() as i64);
                for x in x0..=x1 {
                    for y in y0..=y1 {
                        cells.entry((x, y)).or_default().push((pi as u32, ti as u32));
                    }
                }
            }
        }
        Self { parts, cell, cells }
    }

    fn nearest(&self, p: DVec2, reach: f64, accept: impl Fn(usize) -> bool) -> Option<Hit> {
        let (x0, x1) = (((p.x - reach) / self.cell).floor() as i64, ((p.x + reach) / self.cell).floor() as i64);
        let (y0, y1) = (((p.y - reach) / self.cell).floor() as i64, ((p.y + reach) / self.cell).floor() as i64);
        let mut ids: Vec<(u32, u32)> = Vec::new();
        for x in x0..=x1 {
            for y in y0..=y1 {
                if let Some(list) = self.cells.get(&(x, y)) {
                    ids.extend(
                        list.iter()
                            .copied()
                            .filter(|(pi, _)| self.parts.get(*pi as usize).is_some_and(|part| accept(part.set))),
                    );
                }
            }
        }
        ids.sort_unstable();
        ids.dedup();
        let mut hits: Vec<Hit> = Vec::new();
        for (pi, ti) in ids {
            let Some(part) = self.parts.get(pi as usize) else { continue };
            let Some(t) = part.tris.get(ti as usize) else { continue };
            let [a, b, c] = t.map(|i| i as usize);
            let (Some(&pa), Some(&pb), Some(&pc)) = (part.points.get(a), part.points.get(b), part.points.get(c)) else {
                continue;
            };
            let (dist, w) = closest_on_triangle(p, pa, pb, pc);
            if dist > reach {
                continue;
            }
            let z = pa.z * w[0] + pb.z * w[1] + pc.z * w[2];
            let zero = [0.0; BLEND_CHANNELS];
            let (va, vb, vc) = (
                part.values.get(a).copied().unwrap_or(zero),
                part.values.get(b).copied().unwrap_or(zero),
                part.values.get(c).copied().unwrap_or(zero),
            );
            let value = std::array::from_fn(|ch| va[ch] * w[0] + vb[ch] * w[1] + vc[ch] * w[2]);
            hits.push(Hit { dist, z, value });
        }
        let best = hits.iter().map(|h| h.dist).fold(f64::INFINITY, f64::min);
        if !best.is_finite() {
            return None;
        }
        let near: Vec<&Hit> = hits.iter().filter(|h| h.dist <= best + TIE).collect();
        let n = near.len() as f64;
        Some(Hit {
            dist: best,
            z: near.iter().map(|h| h.z).sum::<f64>() / n,
            value: std::array::from_fn(|ch| near.iter().map(|h| h.value[ch]).sum::<f64>() / n),
        })
    }
}

#[derive(Clone, Copy)]
struct Row {
    dist: f64,
    f: f64,
    delta: f64,
    other_value: [f64; BLEND_CHANNELS],
    other_h: f64,
}

pub fn blend_sets(parts: &[BlendPart], params: &BlendParams) -> Vec<BlendResult> {
    let width = if params.width.is_finite() { params.width.clamp(MIN_BLEND_WIDTH, MAX_BLEND_WIDTH) } else { 128.0 };
    let index = TriIndex::new(parts, width);
    let mut rows: Vec<Vec<Option<Row>>> = Vec::with_capacity(parts.len());
    let mut interface: Vec<(DVec2, f64, usize)> = Vec::new();
    for part in parts {
        let mut list = Vec::with_capacity(part.points.len());
        for p in &part.points {
            let row = index
                .nearest(p.truncate(), width, |other| params.mode.accepts(part.set, other))
                .filter(|_| p.is_finite())
                .map(|hit| {
                    let target = params.mode.target(p.z, hit.z);
                    Row {
                        dist: hit.dist,
                        f: params.falloff.eval(1.0 - hit.dist / width),
                        delta: target - p.z,
                        other_value: hit.value,
                        other_h: hit.z,
                    }
                });
            if let Some(r) = row
                && r.dist <= TOUCH
            {
                interface.push((p.truncate(), r.delta, part.set));
            }
            list.push(row);
        }
        rows.push(list);
    }
    parts
        .iter()
        .zip(&rows)
        .map(|(part, list)| {
            let mut out = BlendResult {
                heights: part.points.iter().map(|p| p.z).collect(),
                values: part.values.clone(),
                influence: vec![0.0; part.points.len()],
            };
            out.values.resize(part.points.len(), [0.0; BLEND_CHANNELS]);
            for (k, row) in list.iter().enumerate() {
                let (Some(row), Some(p)) = (row, part.points.get(k)) else { continue };
                if row.f <= 0.0 {
                    continue;
                }
                let mut delta = row.delta;
                if params.match_edges && row.dist > TOUCH {
                    let xy = p.truncate();
                    let nearest = interface
                        .iter()
                        .filter(|(_, _, set)| *set == part.set)
                        .min_by(|a, b| a.0.distance_squared(xy).total_cmp(&b.0.distance_squared(xy)));
                    if let Some((_, d, _)) = nearest {
                        delta = *d;
                    }
                }
                if let Some(h) = out.heights.get_mut(k) {
                    *h = p.z + row.f * delta;
                }
                if let Some(slot) = out.influence.get_mut(k) {
                    *slot = row.f;
                }
                if params.alpha
                    && let Some(own) = out.values.get_mut(k)
                {
                    let span = row.other_h - p.z;
                    let nominal = match params.mode {
                        BlendMode::Max | BlendMode::Min if span.abs() > 1e-9 && !params.match_edges => {
                            (row.delta / span).clamp(0.0, 1.0)
                        }
                        mode => mode.nominal(),
                    };
                    let w = row.f * nominal;
                    for (v, o) in own.iter_mut().zip(row.other_value) {
                        *v += (o - *v) * w;
                    }
                }
            }
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(x0: f64, z: f64, set: usize) -> BlendPart {
        let n = 9usize;
        let step = 16.0;
        let mut points = Vec::new();
        let mut values = Vec::new();
        for j in 0..n {
            for i in 0..n {
                points.push(DVec3::new(x0 + i as f64 * step, j as f64 * step, z));
                values.push([if set == 0 { 0.0 } else { 255.0 }, 0.0, 0.0, 0.0]);
            }
        }
        let mut tris = Vec::new();
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = (j * n + i) as u32;
                let (b, c, d) = (a + 1, a + n as u32, a + n as u32 + 1);
                tris.push([a, c, b]);
                tris.push([b, c, d]);
            }
        }
        BlendPart { set, points, values, tris }
    }

    fn params(mode: BlendMode, width: f64) -> BlendParams {
        BlendParams { width, falloff: BlendFalloff::Linear, mode, match_edges: false, alpha: true }
    }

    #[test]
    fn falloff_curves_reach_the_ends() {
        for f in BlendFalloff::ALL {
            assert_eq!(f.eval(0.0), 0.0);
            assert!((f.eval(1.0) - 1.0).abs() < 1e-12);
            assert!(f.eval(0.5) > 0.0 && f.eval(0.5) < 1.0);
        }
        assert!(BlendFalloff::EaseIn.eval(0.5) < BlendFalloff::Linear.eval(0.5));
        assert!(BlendFalloff::EaseOut.eval(0.5) > BlendFalloff::Linear.eval(0.5));
    }

    #[test]
    fn adjacent_tiles_meet_on_their_shared_edge() {
        let parts = [tile(0.0, 0.0, 0), tile(128.0, 100.0, 1)];
        let out = blend_sets(&parts, &params(BlendMode::Average, 64.0));
        for j in 0..9 {
            let a = out[0].heights[j * 9 + 8];
            let b = out[1].heights[j * 9];
            assert!((a - 50.0).abs() < 1e-9 && (b - 50.0).abs() < 1e-9, "{a} {b}");
            assert!((out[0].values[j * 9 + 8][0] - out[1].values[j * 9][0]).abs() < 1e-9);
        }
        assert_eq!(out[0].heights[4], 0.0);
        assert_eq!(out[1].heights[4 * 9 + 8], 100.0);
        let ramp: Vec<f64> = (4..=8).map(|i| out[0].heights[i]).collect();
        assert!(ramp.windows(2).all(|w| w[1] >= w[0]), "{ramp:?}");
        assert!((out[0].heights[7] - 37.5).abs() < 1e-9);
    }

    #[test]
    fn a_over_b_keeps_a_and_pulls_b_to_it() {
        let parts = [tile(0.0, 0.0, 0), tile(128.0, 100.0, 1)];
        let out = blend_sets(&parts, &params(BlendMode::AOverB, 64.0));
        assert!(out[0].heights.iter().all(|h| *h == 0.0));
        assert!(out[1].heights[0].abs() < 1e-9);
        assert_eq!(out[1].heights[8], 100.0);
        let back = blend_sets(&parts, &params(BlendMode::BOverA, 64.0));
        assert!((back[0].heights[8] - 100.0).abs() < 1e-9);
        assert!(back[1].heights.iter().all(|h| *h == 100.0));
    }

    #[test]
    fn max_and_min_pick_the_taller_or_lower_side() {
        let parts = [tile(0.0, 0.0, 0), tile(128.0, 100.0, 1)];
        let max = blend_sets(&parts, &params(BlendMode::Max, 64.0));
        assert!((max[0].heights[8] - 100.0).abs() < 1e-9 && (max[1].heights[0] - 100.0).abs() < 1e-9);
        let min = blend_sets(&parts, &params(BlendMode::Min, 64.0));
        assert!(min[0].heights[8].abs() < 1e-9 && min[1].heights[0].abs() < 1e-9);
    }

    #[test]
    fn match_edges_keeps_the_relief_of_each_side() {
        let mut parts = [tile(0.0, 0.0, 0), tile(128.0, 100.0, 1)];
        for (k, p) in parts[0].points.iter_mut().enumerate() {
            p.z += (k % 3) as f64 * 2.0;
        }
        let mut with = params(BlendMode::Average, 64.0);
        with.match_edges = true;
        let plain = blend_sets(&parts, &params(BlendMode::Average, 64.0));
        let kept = blend_sets(&parts, &with);
        let k = 4 * 9 + 6;
        let relief_before = parts[0].points[k].z - parts[0].points[k - 1].z;
        let relief_plain = plain[0].heights[k] - plain[0].heights[k - 1];
        let relief_kept = kept[0].heights[k] - kept[0].heights[k - 1];
        assert!((relief_kept - relief_before).abs() <= (relief_plain - relief_before).abs() + 1e-9);
        for j in 0..9 {
            assert!((kept[0].heights[j * 9 + 8] - kept[1].heights[j * 9]).abs() < 1e-9);
        }
    }

    #[test]
    fn overlapping_tiles_agree_and_distant_vertices_are_untouched() {
        let parts = [tile(0.0, 0.0, 0), tile(64.0, 80.0, 1)];
        let out = blend_sets(&parts, &params(BlendMode::Average, 32.0));
        let a = out[0].heights[4 * 9 + 6];
        let b = out[1].heights[4 * 9 + 2];
        assert!((a - 40.0).abs() < 1e-9 && (b - 40.0).abs() < 1e-9, "{a} {b}");
        assert_eq!(out[0].heights[4 * 9], 0.0);
        assert_eq!(out[1].heights[4 * 9 + 8], 80.0);
    }

    #[test]
    fn three_sets_blend_with_their_neighbours() {
        let parts = [tile(0.0, 0.0, 0), tile(128.0, 60.0, 1), tile(256.0, 120.0, 2)];
        let out = blend_sets(&parts, &params(BlendMode::Average, 32.0));
        assert!((out[0].heights[8] - 30.0).abs() < 1e-9);
        assert!((out[1].heights[0] - 30.0).abs() < 1e-9 && (out[1].heights[8] - 90.0).abs() < 1e-9);
        assert!((out[2].heights[0] - 90.0).abs() < 1e-9);
    }
}
