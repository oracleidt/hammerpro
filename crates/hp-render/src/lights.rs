use glam::Vec3;
use rayon::prelude::*;

use crate::camera::Camera;
use crate::gpu::LightGpu;
use crate::types::{LightFalloff, LightKind, PreviewLight};

pub(crate) const MAX_LIGHTS: usize = 1 << 16;
pub(crate) const MAX_DIRECTIONAL: usize = 8;
pub(crate) const TILE_PX: u32 = 64;
pub(crate) const SLICES: u32 = 16;
pub const MAX_PER_CLUSTER: u32 = 128;
const MAX_REFS: usize = 1 << 21;
const CLUSTER_FAR: f32 = 32768.0;

pub(crate) const KIND_POINT: f32 = 0.0;
pub(crate) const KIND_SPOT: f32 = 1.0;
pub(crate) const KIND_DIRECTIONAL: f32 = 2.0;

pub(crate) const DLIGHT_FLAG: f32 = 2.0;

pub(crate) fn is_dlight(light: &LightGpu) -> bool {
    light.extra[1] > 1.5
}

const DEFAULT_ATTEN: [f32; 4] = [0.0, 0.0, 1.0e-4, 1.0e9];

fn gpu_cone(f: Option<&LightFalloff>) -> [f32; 4] {
    match f {
        Some(f) => {
            let e = if f.exponent.is_finite() { f.exponent.clamp(0.0, 64.0) } else { 0.0 };
            [e, if f.dlight { DLIGHT_FLAG } else { 1.0 }, 0.0, 0.0]
        }
        None => [0.0; 4],
    }
}

fn gpu_falloff(f: Option<&LightFalloff>) -> ([f32; 4], f32, [f32; 2]) {
    let Some(f) = f else { return (DEFAULT_ATTEN, 0.0, [0.0; 2]) };
    let term = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0e9) } else { 0.0 };
    let (c, l, q) = (term(f.constant), term(f.linear), term(f.quadratic));
    if c + l + q <= 0.0 {
        return (DEFAULT_ATTEN, 0.0, [0.0; 2]);
    }
    let cap = if f.cap.is_finite() && f.cap >= 1.0 { f.cap.min(1.0e9) } else { 1.0e9 };
    let cutoff = if f.cutoff.is_finite() { f.cutoff.clamp(0.0, 1.0e9) } else { 0.0 };
    let fade =
        if f.fade.iter().all(|v| v.is_finite() && *v >= 0.0) && f.fade[1] > f.fade[0] { f.fade } else { [0.0; 2] };
    ([c, l, q, cap], cutoff, fade)
}

fn finite3(v: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    if v.iter().all(|c| c.is_finite()) { v } else { fallback }
}

pub(crate) struct Packed {
    pub lights: Vec<LightGpu>,
    pub n_dir: u32,
    pub left_out: u32,
}

pub(crate) fn pack_lights(lights: &[PreviewLight]) -> Packed {
    let mut dir = Vec::new();
    let mut local = Vec::new();
    let mut left_out = 0u32;
    for l in lights {
        let directional = matches!(l.kind, LightKind::Directional { .. });
        let full = if directional { dir.len() >= MAX_DIRECTIONAL } else { local.len() + MAX_DIRECTIONAL >= MAX_LIGHTS };
        if full {
            left_out += 1;
            continue;
        }
        let intensity = if l.intensity.is_finite() { l.intensity.max(0.0) } else { 0.0 };
        let c = finite3(l.color, [0.0; 3]);
        let color = [c[0].max(0.0) * intensity, c[1].max(0.0) * intensity, c[2].max(0.0) * intensity];
        let pos = finite3(l.pos, [0.0; 3]);
        let range = if l.range.is_finite() { l.range.clamp(1.0, 1.0e6) } else { 1.0 };
        let (atten, cutoff, fade) = gpu_falloff(l.falloff.as_ref());
        let extra = gpu_cone(l.falloff.as_ref());
        match l.kind {
            LightKind::Point => local.push(LightGpu {
                pos_range: [pos[0], pos[1], pos[2], range],
                color: [color[0], color[1], color[2], KIND_POINT],
                dir: [0.0, 0.0, -1.0, cutoff],
                cone: [-1.0, -1.0, fade[0], fade[1]],
                atten,
                extra,
                delta_color: [0.0; 4],
            }),
            LightKind::Spot { dir: d, inner_deg, outer_deg } => {
                let d = Vec3::from(finite3(d, [0.0, 0.0, -1.0])).normalize_or(Vec3::NEG_Z);
                let outer = if outer_deg.is_finite() { outer_deg.clamp(0.1, 179.0) } else { 45.0 };
                let inner = if inner_deg.is_finite() { inner_deg.clamp(0.0, outer) } else { outer * 0.5 };
                let (ci, co) = (inner.to_radians().cos(), outer.to_radians().cos());
                let ci = ci.max(co + 1e-4);
                local.push(LightGpu {
                    pos_range: [pos[0], pos[1], pos[2], range],
                    color: [color[0], color[1], color[2], KIND_SPOT],
                    dir: [d.x, d.y, d.z, cutoff],
                    cone: [ci, co, fade[0], fade[1]],
                    atten,
                    extra,
                    delta_color: [0.0; 4],
                });
            }
            LightKind::Directional { dir: d } => {
                let d = Vec3::from(finite3(d, [0.0, 0.0, -1.0])).normalize_or(Vec3::NEG_Z);
                dir.push(LightGpu {
                    pos_range: [0.0; 4],
                    color: [color[0], color[1], color[2], KIND_DIRECTIONAL],
                    dir: [d.x, d.y, d.z, 0.0],
                    cone: [0.0; 4],
                    atten: DEFAULT_ATTEN,
                    extra: [0.0; 4],
                    delta_color: [0.0; 4],
                });
            }
        }
    }
    let n_dir = dir.len() as u32;
    dir.extend(local);
    Packed { lights: dir, n_dir, left_out }
}

pub(crate) fn luminance(c: [f32; 4]) -> f32 {
    c[0] * 0.3 + c[1] * 0.59 + c[2] * 0.11
}

pub(crate) fn light_score(l: &LightGpu, at: Vec3) -> f32 {
    let pos = Vec3::new(l.pos_range[0], l.pos_range[1], l.pos_range[2]);
    let d = pos.distance(at).max(1.0);
    let [c, lin, q, cap] = l.atten;
    let dc = d.min(cap.max(1.0));
    let denom = c + dc * (lin + dc * q);
    let falloff = if denom > 1e-12 { 1.0 / denom } else { 1.0 };
    let x = d / l.pos_range[3].max(1e-3);
    let window = (1.0 - x * x * x * x).clamp(0.0, 1.0);
    let s = luminance(l.color) * falloff * window * window;
    if s.is_finite() { s.max(0.0) } else { 0.0 }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct ClusterGrid {
    pub tiles_x: u32,
    pub tiles_y: u32,
    pub slices: u32,
    pub near: f32,
    pub far: f32,
    pub scale: f32,
}

impl ClusterGrid {
    pub fn new(cam: &Camera) -> Self {
        let tiles_x = (cam.width as u32).div_ceil(TILE_PX).max(1);
        let tiles_y = (cam.height as u32).div_ceil(TILE_PX).max(1);
        let near = cam.near.max(1e-3);
        let far = cam.far.min(CLUSTER_FAR).max(near * 2.0);
        let scale = SLICES as f32 / (far / near).ln();
        Self { tiles_x, tiles_y, slices: SLICES, near, far, scale }
    }

    pub fn count(&self) -> usize {
        (self.tiles_x * self.tiles_y * self.slices) as usize
    }

    fn slice(&self, depth: f32) -> u32 {
        if depth <= self.near {
            return 0;
        }
        ((depth / self.near).ln() * self.scale).max(0.0).min((self.slices - 1) as f32) as u32
    }

    pub(crate) fn slice_of(&self, depth: f32) -> u32 {
        if depth.is_nan() { 0 } else { self.slice(depth) }
    }

    pub(crate) fn center(&self, cam: &Camera, x: u32, y: u32, s: u32) -> Vec3 {
        let depth = if self.scale > 0.0 { self.near * ((s as f32 + 0.5) / self.scale).exp() } else { self.near };
        let px = ((x as f32 + 0.5) * TILE_PX as f32).min(cam.width);
        let py = ((y as f32 + 0.5) * TILE_PX as f32).min(cam.height);
        let nx = px / cam.width.max(1.0) * 2.0 - 1.0;
        let ny = 1.0 - py / cam.height.max(1.0) * 2.0;
        let aspect = cam.width / cam.height.max(1.0);
        let (tx, ty) = (cam.tan_half_fov * aspect, cam.tan_half_fov);
        cam.eye + cam.forward * depth + cam.right * (nx * tx * depth) + cam.up * (ny * ty * depth)
    }
}

pub(crate) fn pack_indices(list: &[u32]) -> Vec<u32> {
    list.chunks(2).map(|pair| (pair[0] & 0xffff) | (pair.get(1).map_or(0, |v| v & 0xffff) << 16)).collect()
}

pub(crate) struct Clusters {
    pub table: Vec<[u32; 2]>,
    pub list: Vec<u32>,
    pub crowded: u32,
}

#[derive(Clone, Copy, Debug)]
struct Cell {
    light: u32,
    slice: u32,
    rect: [u32; 4],
}

fn slab(grid: &ClusterGrid, slice: u32) -> (f32, f32) {
    let edge = |s: u32| grid.near * (s as f32 / grid.scale.max(1e-6)).exp();
    let z0 = if slice == 0 { 0.0 } else { edge(slice) * 0.999 };
    let z1 = if slice + 1 >= grid.slices { f32::MAX } else { edge(slice + 1) * 1.001 };
    (z0, z1)
}

fn cluster_cells(
    cam: &Camera,
    grid: &ClusterGrid,
    lights: &[LightGpu],
    first_local: u32,
    delta_only: bool,
) -> Vec<Cell> {
    let aspect = cam.width / cam.height;
    let (tx, ty) = (cam.tan_half_fov * aspect, cam.tan_half_fov);
    let mut cells: Vec<Cell> = Vec::new();
    for (i, l) in lights.iter().enumerate().skip(first_local as usize) {
        if delta_only && !crate::live_lighting::has_delta(l) {
            continue;
        }
        let p = Vec3::new(l.pos_range[0], l.pos_range[1], l.pos_range[2]);
        let r = l.pos_range[3];
        let d = p - cam.eye;
        let (vx, vy, vz) = (d.dot(cam.right), d.dot(cam.up), d.dot(cam.forward));
        if vz + r < grid.near || vz - r > cam.far {
            continue;
        }
        let (s0, s1) = (grid.slice(vz - r), grid.slice(vz + r));
        for s in s0..=s1 {
            let (z0, z1) = slab(grid, s);
            let dz = if vz < z0 {
                z0 - vz
            } else if vz > z1 {
                vz - z1
            } else {
                0.0
            };
            if dz >= r {
                continue;
            }
            let rz = (r * r - dz * dz).sqrt();
            let (zn, zf) = (z0.max(vz - r), z1.min(vz + r));
            let rect = if zn <= grid.near {
                [0, grid.tiles_x - 1, 0, grid.tiles_y - 1]
            } else {
                let lo = |v: f32| if v < 0.0 { v / zn } else { v / zf };
                let hi = |v: f32| if v > 0.0 { v / zn } else { v / zf };
                let nx0 = lo(vx - rz) / tx;
                let nx1 = hi(vx + rz) / tx;
                let ny0 = lo(vy - rz) / ty;
                let ny1 = hi(vy + rz) / ty;
                if nx1 < -1.0 || nx0 > 1.0 || ny1 < -1.0 || ny0 > 1.0 {
                    continue;
                }
                let to_px_x = |nx: f32| ((nx.clamp(-1.0, 1.0) + 1.0) * 0.5 * cam.width) as u32 / TILE_PX;
                let to_px_y = |ny: f32| ((1.0 - ny.clamp(-1.0, 1.0)) * 0.5 * cam.height) as u32 / TILE_PX;
                [
                    to_px_x(nx0).min(grid.tiles_x - 1),
                    to_px_x(nx1).min(grid.tiles_x - 1),
                    to_px_y(ny1).min(grid.tiles_y - 1),
                    to_px_y(ny0).min(grid.tiles_y - 1),
                ]
            };
            cells.push(Cell { light: i as u32, slice: s, rect });
        }
    }
    cells
}

fn cell_refs(cells: &[Cell]) -> usize {
    cells
        .iter()
        .map(|c| {
            let [x0, x1, y0, y1] = c.rect;
            (x1 + 1).saturating_sub(x0) as usize * (y1 + 1).saturating_sub(y0) as usize
        })
        .sum()
}

pub(crate) fn build_clusters(
    cam: &Camera,
    grid: &ClusterGrid,
    lights: &[LightGpu],
    first_local: u32,
    delta_only: bool,
) -> Clusters {
    let cells = cluster_cells(cam, grid, lights, first_local, delta_only);
    if cell_refs(&cells) > MAX_REFS || crate::profile::baseline("clusters") {
        return build_serial(cam, grid, lights, &cells, delta_only);
    }
    let (tx, plane) = (grid.tiles_x as usize, (grid.tiles_x * grid.tiles_y) as usize);
    let mut by_slice: Vec<Vec<Cell>> = vec![Vec::new(); grid.slices as usize];
    for c in &cells {
        if let Some(v) = by_slice.get_mut(c.slice as usize) {
            v.push(*c);
        }
    }
    let parts: Vec<(Vec<u32>, Vec<u32>)> = by_slice
        .par_iter()
        .map(|ids| {
            let mut counts = vec![0u32; plane];
            for c in ids {
                let [x0, x1, y0, y1] = c.rect;
                for y in y0 as usize..=y1 as usize {
                    for n in &mut counts[y * tx + x0 as usize..=y * tx + x1 as usize] {
                        *n += 1;
                    }
                }
            }
            let mut offsets = vec![0u32; plane];
            let mut total = 0u32;
            for (o, n) in offsets.iter_mut().zip(&counts) {
                *o = total;
                total += n;
            }
            let mut list = vec![0u32; total as usize];
            let mut fill = offsets;
            for c in ids {
                let [x0, x1, y0, y1] = c.rect;
                for y in y0 as usize..=y1 as usize {
                    for at in y * tx + x0 as usize..=y * tx + x1 as usize {
                        list[fill[at] as usize] = c.light;
                        fill[at] += 1;
                    }
                }
            }
            (counts, list)
        })
        .collect();
    let mut table = vec![[0u32; 2]; grid.count()];
    let mut list: Vec<u32> = Vec::with_capacity(parts.iter().map(|(_, l)| l.len()).sum());
    for (s, (counts, part)) in parts.iter().enumerate() {
        let mut offset = list.len() as u32;
        for (at, n) in counts.iter().enumerate() {
            table[s * plane + at] = [offset, *n];
            offset += n;
        }
        list.extend_from_slice(part);
    }
    finish_clusters(cam, grid, lights, table, list, delta_only)
}

fn build_serial(cam: &Camera, grid: &ClusterGrid, lights: &[LightGpu], cells: &[Cell], delta_only: bool) -> Clusters {
    let n = grid.count();
    let mut counts = vec![0u32; n];
    let idx = |x: u32, y: u32, s: u32| ((s * grid.tiles_y + y) * grid.tiles_x + x) as usize;
    let mut total = 0usize;
    for c in cells {
        let [x0, x1, y0, y1] = c.rect;
        for y in y0..=y1 {
            for x in x0..=x1 {
                if let Some(n) = counts.get_mut(idx(x, y, c.slice))
                    && total < MAX_REFS
                {
                    *n += 1;
                    total += 1;
                }
            }
        }
    }
    let mut table = vec![[0u32; 2]; n];
    let mut offset = 0u32;
    for (t, c) in table.iter_mut().zip(&counts) {
        *t = [offset, 0];
        offset += c;
    }
    let mut list = vec![0u32; total];
    for c in cells {
        let [x0, x1, y0, y1] = c.rect;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let ci = idx(x, y, c.slice);
                let (Some(t), Some(&cap)) = (table.get_mut(ci), counts.get(ci)) else { continue };
                if t[1] < cap
                    && let Some(slot) = list.get_mut((t[0] + t[1]) as usize)
                {
                    *slot = c.light;
                    t[1] += 1;
                }
            }
        }
    }
    finish_clusters(cam, grid, lights, table, list, delta_only)
}

fn finish_clusters(
    cam: &Camera,
    grid: &ClusterGrid,
    lights: &[LightGpu],
    mut table: Vec<[u32; 2]>,
    mut list: Vec<u32>,
    delta_only: bool,
) -> Clusters {
    let plane = grid.tiles_x * grid.tiles_y;
    let mut jobs: Vec<(u32, &mut [u32], &mut [u32; 2])> = Vec::new();
    let mut rest: &mut [u32] = &mut list;
    let mut used = 0u32;
    for (ci, t) in table.iter_mut().enumerate() {
        let skip = (t[0] - used) as usize;
        let (_, tail) = std::mem::take(&mut rest).split_at_mut(skip.min(rest.len()));
        let take = (t[1] as usize).min(tail.len());
        let (slice, tail) = tail.split_at_mut(take);
        rest = tail;
        used = t[0] + take as u32;
        if t[1] > 0 {
            jobs.push((ci as u32, slice, t));
        }
    }
    let crowded = jobs.iter().filter(|(_, _, t)| t[1] > MAX_PER_CLUSTER).count() as u32;
    let tighten = !crate::profile::baseline("clusterbounds");
    let view_lights: Vec<Vec3> = lights
        .iter()
        .map(|l| {
            let delta = Vec3::from_slice(&l.pos_range) - cam.eye;
            Vec3::new(delta.dot(cam.right), delta.dot(cam.up), delta.dot(cam.forward))
        })
        .collect();
    jobs.into_par_iter().for_each(|(ci, slice, t)| {
        let (s, rest) = (ci / plane, ci % plane);
        let (x, y) = (rest % grid.tiles_x, rest / grid.tiles_x);
        if t[1] > MAX_PER_CLUSTER {
            let center = grid.center(cam, x, y, s);
            let mut scored: Vec<(f32, u32)> = slice
                .iter()
                .map(|&i| {
                    (
                        lights.get(i as usize).map_or(0.0, |l| {
                            let mut scored = *l;
                            if delta_only {
                                scored.color = l.delta_color.map(f32::abs);
                            }
                            light_score(&scored, center)
                        }),
                        i,
                    )
                })
                .collect();
            let keep = MAX_PER_CLUSTER as usize;
            let order = |a: &(f32, u32), b: &(f32, u32)| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1));
            scored.select_nth_unstable_by(keep - 1, order);
            scored.truncate(keep);
            scored.sort_unstable_by(order);
            for (slot, (_, i)) in slice.iter_mut().zip(scored) {
                *slot = i;
            }
            t[1] = MAX_PER_CLUSTER;
        }
        if tighten {
            let bounds = ClusterBounds::new(cam, grid, x, y, s);
            let mut kept = 0usize;
            for at in 0..t[1] as usize {
                let index = slice[at];
                let visible = lights.get(index as usize).is_none_or(|l| {
                    l.cone[3] > l.cone[2] || bounds.intersects(view_lights[index as usize], l.pos_range[3])
                });
                if visible {
                    slice[kept] = index;
                    kept += 1;
                }
            }
            t[1] = kept as u32;
        }
    });
    let mut end = 0usize;
    for t in &mut table {
        let start = t[0] as usize;
        let count = t[1] as usize;
        list.copy_within(start..start + count, end);
        t[0] = end as u32;
        end += count;
    }
    list.truncate(end);
    if list.capacity() > end.saturating_mul(2) {
        list.shrink_to_fit();
    }
    Clusters { table, list, crowded }
}

pub(crate) struct ClusterBounds {
    x: [f32; 2],
    y: [f32; 2],
    z: [f32; 2],
    lengths: [f32; 4],
}

impl ClusterBounds {
    pub(crate) fn new(cam: &Camera, grid: &ClusterGrid, x: u32, y: u32, slice: u32) -> Self {
        let tx = cam.tan_half_fov * cam.width / cam.height;
        let ty = cam.tan_half_fov;
        let px = |p: f32| (p / cam.width * 2.0 - 1.0) * tx;
        let py = |p: f32| (1.0 - p / cam.height * 2.0) * ty;
        let x = [px((x * TILE_PX) as f32 - 2.0), px(((x + 1) * TILE_PX) as f32 + 2.0)];
        let y = [py(((y + 1) * TILE_PX) as f32 + 2.0), py((y * TILE_PX) as f32 - 2.0)];
        let (z0, z1) = slab(grid, slice);
        let lengths = [x[0], x[1], y[0], y[1]].map(|v| (1.0 + v * v).sqrt());
        Self { x, y, z: [z0, z1], lengths }
    }

    pub(crate) fn intersects(&self, p: Vec3, radius: f32) -> bool {
        let radius = radius + radius.abs() * 1e-5 + 0.01;
        p.z + radius >= self.z[0]
            && p.z - radius <= self.z[1]
            && p.x - self.x[0] * p.z >= -radius * self.lengths[0]
            && self.x[1] * p.z - p.x >= -radius * self.lengths[1]
            && p.y - self.y[0] * p.z >= -radius * self.lengths[2]
            && self.y[1] * p.z - p.y >= -radius * self.lengths[3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{RenderMode, ViewKind, ViewParams};

    fn cam() -> Camera {
        Camera::new(&ViewParams {
            game_pixel_grid: false,
            size: [640, 480],
            kind: ViewKind::Perspective {
                eye: [0.0; 3],
                forward: [1.0, 0.0, 0.0],
                up: [0.0, 0.0, 1.0],
                fov_y_deg: 90.0,
                near: 1.0,
                far: 10000.0,
            },
            mode: RenderMode::Lit,
            grid: None,
            background: [0.0; 3],
            selection_color: [1.0; 4],
            show_sprites: false,
            show_models: false,
            msaa: false,
        })
    }

    fn point(pos: [f32; 3], range: f32) -> PreviewLight {
        PreviewLight { kind: LightKind::Point, pos, color: [1.0; 3], intensity: 1.0, range, falloff: None }
    }

    #[test]
    fn directional_first_and_capped() {
        let mut lights = vec![point([0.0; 3], 100.0)];
        for _ in 0..20 {
            lights.push(PreviewLight {
                kind: LightKind::Directional { dir: [0.0, 0.0, -1.0] },
                pos: [0.0; 3],
                color: [1.0; 3],
                intensity: 1.0,
                range: 0.0,
                falloff: None,
            });
        }
        let p = pack_lights(&lights);
        assert_eq!(p.n_dir as usize, MAX_DIRECTIONAL);
        assert_eq!(p.lights.len(), MAX_DIRECTIONAL + 1);
        assert_eq!(p.lights[MAX_DIRECTIONAL].color[3], KIND_POINT);
        assert_eq!(p.left_out, 20 - MAX_DIRECTIONAL as u32);
        let many: Vec<PreviewLight> = (0..5000).map(|_| point([0.0; 3], 1.0)).collect();
        let p = pack_lights(&many);
        assert_eq!((p.lights.len(), p.left_out), (5000, 0), "big maps keep every light");
        let too_many: Vec<PreviewLight> = (0..MAX_LIGHTS + 10).map(|_| point([0.0; 3], 1.0)).collect();
        let p = pack_lights(&too_many);
        assert_eq!(p.lights.len() + p.left_out as usize, MAX_LIGHTS + 10, "lights left out are counted");
        let mut sun_last = many.clone();
        sun_last.push(PreviewLight { kind: LightKind::Directional { dir: [0.0, 0.0, -1.0] }, ..point([0.0; 3], 1.0) });
        assert_eq!(pack_lights(&sun_last).n_dir, 1, "the sun is kept whatever its place in the list");
    }

    #[test]
    fn crowded_clusters_keep_the_strongest_lights() {
        let cam = cam();
        let grid = ClusterGrid::new(&cam);
        let mut lights: Vec<PreviewLight> = (0..MAX_PER_CLUSTER as usize + 40)
            .map(|i| PreviewLight { intensity: 0.01, ..point([500.0, (i as f32) * 0.01, 0.0], 400.0) })
            .collect();
        lights.push(PreviewLight { intensity: 50.0, ..point([500.0, 0.0, 0.0], 400.0) });
        let p = pack_lights(&lights);
        let c = build_clusters(&cam, &grid, &p.lights, p.n_dir, false);
        assert!(c.crowded > 0, "the clusters around the lights are crowded");
        assert!(c.table.iter().all(|t| t[1] <= MAX_PER_CLUSTER));
        let strong = (lights.len() - 1) as u32;
        let s = grid.slice(500.0);
        let center = ((s * grid.tiles_y + grid.tiles_y / 2) * grid.tiles_x + grid.tiles_x / 2) as usize;
        let t = c.table[center];
        assert_eq!(t[1], MAX_PER_CLUSTER);
        assert!(c.list[t[0] as usize..(t[0] + t[1]) as usize].contains(&strong), "the brightest light is kept");
        let few = pack_lights(&lights[..10]);
        assert_eq!(build_clusters(&cam, &grid, &few.lights, few.n_dir, false).crowded, 0);
    }

    #[test]
    fn light_in_front_is_assigned_to_center_clusters() {
        let cam = cam();
        let grid = ClusterGrid::new(&cam);
        let p = pack_lights(&[point([500.0, 0.0, 0.0], 50.0), point([-500.0, 0.0, 0.0], 50.0)]);
        let Clusters { table, list, .. } = build_clusters(&cam, &grid, &p.lights, p.n_dir, false);
        assert_eq!(table.len(), grid.count());
        assert!(!list.is_empty());
        assert!(list.iter().all(|&i| i == 0));
        let s = grid.slice(500.0);
        let center = ((s * grid.tiles_y + grid.tiles_y / 2) * grid.tiles_x + grid.tiles_x / 2) as usize;
        assert_eq!(table[center][1], 1);
        assert_eq!(table[(s * grid.tiles_y * grid.tiles_x) as usize][1], 0);
    }

    #[test]
    fn light_around_camera_covers_screen() {
        let cam = cam();
        let grid = ClusterGrid::new(&cam);
        let p = pack_lights(&[point([0.0; 3], 200.0)]);
        let table = build_clusters(&cam, &grid, &p.lights, p.n_dir, false).table;
        assert_eq!(table[0][1], 1);
        assert_eq!(table[(grid.tiles_x * grid.tiles_y - 1) as usize][1], 1);
    }

    #[test]
    fn hostile_values_do_not_panic() {
        let cam = cam();
        let grid = ClusterGrid::new(&cam);
        let lights = [PreviewLight {
            kind: LightKind::Spot { dir: [f32::NAN; 3], inner_deg: f32::NAN, outer_deg: -5.0 },
            pos: [f32::INFINITY, 0.0, 0.0],
            color: [f32::NAN; 3],
            intensity: f32::NAN,
            range: f32::NAN,
            falloff: None,
        }];
        let p = pack_lights(&lights);
        assert!(p.lights.iter().all(|l| l.pos_range.iter().all(|v| v.is_finite())));
        let _ = build_clusters(&cam, &grid, &p.lights, p.n_dir, false);
    }

    #[test]
    fn vrad_falloff_terms_reach_the_gpu() {
        let f = LightFalloff {
            constant: 0.0,
            linear: 1.0,
            quadratic: 0.5,
            cap: 4000.0,
            cutoff: 900.0,
            fade: [100.0, 600.0],
            exponent: 2.0,
            dlight: false,
        };
        let mut l = point([0.0; 3], 1000.0);
        l.falloff = Some(f);
        let packed = pack_lights(&[l]).lights;
        assert_eq!(packed[0].atten, [0.0, 1.0, 0.5, 4000.0]);
        assert_eq!((packed[0].dir[3], packed[0].cone[2], packed[0].cone[3]), (900.0, 100.0, 600.0));
        assert_eq!(packed[0].extra, [2.0, 1.0, 0.0, 0.0]);
        let packed = pack_lights(&[point([0.0; 3], 10.0)]).lights;
        assert_eq!(packed[0].atten, DEFAULT_ATTEN);
        let hostile = LightFalloff {
            constant: f32::NAN,
            linear: -1.0,
            quadratic: f32::INFINITY,
            cap: -5.0,
            cutoff: f32::NAN,
            fade: [5.0, 1.0],
            exponent: f32::NAN,
            dlight: false,
        };
        assert_eq!(gpu_falloff(Some(&hostile)), (DEFAULT_ATTEN, 0.0, [0.0; 2]));
        assert_eq!(gpu_cone(Some(&hostile)), [0.0, 1.0, 0.0, 0.0]);
        assert_eq!(gpu_cone(None), [0.0; 4]);
        let zero = LightFalloff::default();
        assert_eq!(gpu_falloff(Some(&zero)).0, DEFAULT_ATTEN, "all zero terms would divide by zero");
        let capped = LightFalloff { quadratic: 1.0, cap: f32::NAN, ..zero };
        assert_eq!(gpu_falloff(Some(&capped)).0, [0.0, 0.0, 1.0, 1.0e9]);
    }

    fn scattered(count: usize) -> Packed {
        let mut lights: Vec<PreviewLight> = Vec::new();
        let mut seed = 12345u32;
        let mut next = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / 16_777_216.0
        };
        for _ in 0..count {
            let pos = [next() * 6000.0 - 500.0, next() * 4000.0 - 2000.0, next() * 1000.0 - 300.0];
            lights.push(point(pos, 50.0 + next() * 2500.0));
        }
        pack_lights(&lights)
    }

    #[test]
    fn the_parallel_build_matches_the_serial_one() {
        let c = cam();
        let grid = ClusterGrid::new(&c);
        let packed = scattered(400);
        let first = packed.n_dir;
        let fast = build_clusters(&c, &grid, &packed.lights, first, false);
        let cells = cluster_cells(&c, &grid, &packed.lights, first, false);
        let serial = build_serial(&c, &grid, &packed.lights, &cells, false);
        assert_eq!(fast.crowded, serial.crowded);
        assert_eq!(fast.table, serial.table);
        assert_eq!(fast.list, serial.list);
        assert!(fast.list.len() > 100, "the test lights reach clusters");
    }

    #[test]
    fn every_point_a_light_reaches_finds_it_in_its_cluster() {
        let c = cam();
        let grid = ClusterGrid::new(&c);
        let packed = scattered(60);
        let first = packed.n_dir;
        let built = build_clusters(&c, &grid, &packed.lights, first, false);
        assert_eq!(built.crowded, 0);
        let mut seed = 777u32;
        let mut next = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 8) as f32 / 16_777_216.0
        };
        let mut checked = 0;
        for (i, l) in packed.lights.iter().enumerate().skip(first as usize) {
            let center = Vec3::new(l.pos_range[0], l.pos_range[1], l.pos_range[2]);
            for _ in 0..200 {
                let dir = Vec3::new(next() * 2.0 - 1.0, next() * 2.0 - 1.0, next() * 2.0 - 1.0).normalize_or_zero();
                let p = center + dir * (next() * l.pos_range[3] * 0.999);
                let clip = c.view_proj * p.extend(1.0);
                let depth = (p - c.eye).dot(c.forward);
                if clip.w <= 1e-3 || depth <= grid.near || depth >= grid.far {
                    continue;
                }
                let (nx, ny) = (clip.x / clip.w, clip.y / clip.w);
                if nx.abs() >= 1.0 || ny.abs() >= 1.0 {
                    continue;
                }
                let px = (nx + 1.0) * 0.5 * c.width;
                let py = (1.0 - ny) * 0.5 * c.height;
                let (x, y) = ((px as u32 / TILE_PX).min(grid.tiles_x - 1), (py as u32 / TILE_PX).min(grid.tiles_y - 1));
                let at = ((grid.slice(depth) * grid.tiles_y + y) * grid.tiles_x + x) as usize;
                let [offset, count] = built.table[at];
                let found = built.list[offset as usize..(offset + count) as usize].contains(&(i as u32));
                assert!(found, "light {i} misses a point {depth} units deep it lights");
                checked += 1;
            }
        }
        assert!(checked > 500, "{checked} points checked");
    }
}
