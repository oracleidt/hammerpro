use glam::DVec3;

mod build;
mod conform;
mod sample;
mod select;
#[cfg(test)]
mod tests;

pub use build::MeshDispResult;
pub use conform::{ConformOptions, ConformResult};
pub use select::{TileSelectionOptions, TileSelectionResult};

use sample::{LocalTri, Sampler};

pub const MAX_TILES: usize = 4096;
pub const MAX_NODES: usize = 3_000_000;
pub const MAX_SMOOTH: u32 = 64;
pub const MAX_MARGIN: u32 = 8;
pub const MIN_TILE: f64 = 16.0;
pub const MAX_TILE: f64 = 4096.0;
pub const DISP_LIMIT: usize = 2048;
const SNAP: f64 = 1e-6;

pub type Tri = [DVec3; 3];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AxisChoice {
    #[default]
    Z,
    X,
    Y,
    Best,
}

impl AxisChoice {
    pub const ALL: [AxisChoice; 4] = [AxisChoice::Z, AxisChoice::X, AxisChoice::Y, AxisChoice::Best];

    pub fn id(self) -> &'static str {
        match self {
            AxisChoice::Z => "z",
            AxisChoice::X => "x",
            AxisChoice::Y => "y",
            AxisChoice::Best => "best",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.id().eq_ignore_ascii_case(s.trim()))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PowerChoice {
    #[default]
    Auto,
    Fixed(u32),
}

impl PowerChoice {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.eq_ignore_ascii_case("auto") {
            return Some(PowerChoice::Auto);
        }
        s.parse::<u32>().ok().filter(|p| (2..=4).contains(p)).map(PowerChoice::Fixed)
    }

    pub fn id(self) -> String {
        match self {
            PowerChoice::Auto => "auto".to_owned(),
            PowerChoice::Fixed(p) => p.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Pick {
    #[default]
    Top,
    First,
}

impl Pick {
    pub fn id(self) -> &'static str {
        match self {
            Pick::Top => "top",
            Pick::First => "first",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Pick::Top, Pick::First].into_iter().find(|p| p.id().eq_ignore_ascii_case(s.trim()))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fill {
    #[default]
    Nearest,
    Base,
}

impl Fill {
    pub fn id(self) -> &'static str {
        match self {
            Fill::Nearest => "nearest",
            Fill::Base => "base",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Fill::Nearest, Fill::Base].into_iter().find(|p| p.id().eq_ignore_ascii_case(s.trim()))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Origin {
    #[default]
    Center,
    MinCorner,
    Keep,
}

impl Origin {
    pub fn id(self) -> &'static str {
        match self {
            Origin::Center => "center",
            Origin::MinCorner => "min_corner",
            Origin::Keep => "keep",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        [Origin::Center, Origin::MinCorner, Origin::Keep].into_iter().find(|p| p.id().eq_ignore_ascii_case(s.trim()))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshDispOptions {
    pub axis: AxisChoice,
    pub flip: bool,
    pub tile_size: f64,
    pub power: PowerChoice,
    pub pick: Pick,
    pub fill: Fill,
    pub margin: u32,
    pub smooth: u32,
    pub scale: f64,
    pub origin: Origin,
    pub position: DVec3,
    pub material: String,
    pub uv_scale: f64,
    pub skirt: bool,
    pub bottom: bool,
    pub base_height: Option<f64>,
    pub thickness: f64,
    pub group: bool,
}

impl Default for MeshDispOptions {
    fn default() -> Self {
        Self {
            axis: AxisChoice::Z,
            flip: false,
            tile_size: 256.0,
            power: PowerChoice::Auto,
            pick: Pick::Top,
            fill: Fill::Nearest,
            margin: 0,
            smooth: 0,
            scale: 1.0,
            origin: Origin::Center,
            position: DVec3::ZERO,
            material: "DEV/DEV_MEASUREGENERIC01B".to_owned(),
            uv_scale: 0.25,
            skirt: false,
            bottom: false,
            base_height: None,
            thickness: 16.0,
            group: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum MeshDispError {
    #[error("the mesh has no triangle")]
    Empty,
    #[error("no mesh point lies under or over the tiles along this axis")]
    NoHit,
    #[error("{0}")]
    Option(String),
    #[error("{0} tiles are too many (at most {1}): use bigger tiles or a smaller mesh")]
    TooManyTiles(usize, usize),
    #[error("the base height {0} must be below the lowest point of the surface ({1})")]
    BadBase(f64, f64),
}

impl MeshDispError {
    pub fn key(&self) -> &'static str {
        match self {
            MeshDispError::Empty => "mesh_disp.err_empty",
            MeshDispError::NoHit => "mesh_disp.err_no_hit",
            MeshDispError::Option(_) => "mesh_disp.err_option",
            MeshDispError::TooManyTiles(..) => "mesh_disp.err_tiles",
            MeshDispError::BadBase(..) => "mesh_disp.err_base",
        }
    }

    pub fn args(&self) -> Vec<String> {
        match self {
            MeshDispError::Empty | MeshDispError::NoHit => Vec::new(),
            MeshDispError::Option(s) => vec![s.clone()],
            MeshDispError::TooManyTiles(a, b) => vec![a.to_string(), b.to_string()],
            MeshDispError::BadBase(a, b) => vec![a.to_string(), b.to_string()],
        }
    }
}

impl From<MeshDispError> for crate::EditError {
    fn from(e: MeshDispError) -> Self {
        crate::EditError::Invalid(e.to_string())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub a: usize,
    pub b: usize,
    pub c: usize,
    pub sign: f64,
}

impl Frame {
    pub fn new(axis: usize, flip: bool) -> Self {
        let c = axis.min(2);
        Self { a: (c + 1) % 3, b: (c + 2) % 3, c, sign: if flip { -1.0 } else { 1.0 } }
    }

    pub fn local(&self, p: DVec3) -> DVec3 {
        DVec3::new(p[self.a], p[self.b], self.sign * p[self.c])
    }

    pub fn world(&self, l: DVec3) -> DVec3 {
        let mut w = DVec3::ZERO;
        w[self.a] = l.x;
        w[self.b] = l.y;
        w[self.c] = self.sign * l.z;
        w
    }

    pub fn up(&self) -> DVec3 {
        self.world(DVec3::Z)
    }

    pub fn axis(&self) -> usize {
        self.c
    }

    pub fn flipped(&self) -> bool {
        self.sign < 0.0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Warnings {
    pub overhang_nodes: usize,
    pub filled_nodes: usize,
    pub over_limit: bool,
}

#[derive(Clone, Debug)]
pub struct TilePlan {
    pub frame: Frame,
    pub power: u32,
    pub tile: f64,
    pub spacing: f64,
    pub lo_u: f64,
    pub lo_v: f64,
    pub nx: usize,
    pub ny: usize,
    pub kept: Vec<bool>,
    pub heights: Vec<f64>,
    pub plane: f64,
    pub base: f64,
    pub thickness: f64,
    pub skipped: usize,
    pub warnings: Warnings,
}

impl TilePlan {
    pub fn cells(&self) -> usize {
        1usize << self.power
    }

    pub fn width(&self) -> usize {
        self.nx * self.cells() + 1
    }

    pub fn node(&self, gi: usize, gj: usize) -> f64 {
        self.heights.get(gj * self.width() + gi).copied().unwrap_or(self.plane)
    }

    pub fn tile_kept(&self, i: isize, j: isize) -> bool {
        i >= 0
            && j >= 0
            && (i as usize) < self.nx
            && (j as usize) < self.ny
            && self.kept[j as usize * self.nx + i as usize]
    }

    pub fn tile_count(&self) -> usize {
        self.kept.iter().filter(|k| **k).count()
    }

    pub fn tile_rect(&self, i: usize, j: usize) -> (f64, f64, f64, f64) {
        let u0 = self.lo_u + i as f64 * self.tile;
        let v0 = self.lo_v + j as f64 * self.tile;
        (u0, u0 + self.tile, v0, v0 + self.tile)
    }

    pub fn edge_count(&self) -> usize {
        let mut n = 0;
        for j in 0..self.ny as isize {
            for i in 0..self.nx as isize {
                if self.tile_kept(i, j) {
                    for (di, dj) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                        if !self.tile_kept(i + di, j + dj) {
                            n += 1;
                        }
                    }
                }
            }
        }
        n
    }
}

pub struct Prepared {
    pub frame: Frame,
    pub tris: Vec<LocalTri>,
    pub lo: DVec3,
    pub hi: DVec3,
}

fn best_axis(tris: &[Tri]) -> (usize, bool) {
    let mut abs = DVec3::ZERO;
    let mut signed = DVec3::ZERO;
    for t in tris {
        let n = (t[1] - t[0]).cross(t[2] - t[0]) * 0.5;
        abs += n.abs();
        signed += n;
    }
    let axis = if abs.x >= abs.y && abs.x >= abs.z {
        0
    } else if abs.y >= abs.z {
        1
    } else {
        2
    };
    (axis, signed[axis] < -0.01 * abs[axis])
}

pub fn prepare(world: &[Tri], opts: &MeshDispOptions, forced: Option<Frame>) -> Result<Prepared, MeshDispError> {
    if !(0.001..=1000.0).contains(&opts.scale) {
        return Err(MeshDispError::Option("scale must be between 0.001 and 1000".to_owned()));
    }
    let scaled: Vec<Tri> = world
        .iter()
        .map(|t| t.map(|p| p * opts.scale))
        .filter(|t| t.iter().all(|p| p.is_finite() && p.abs().max_element() < 1.0e7))
        .collect();
    if scaled.is_empty() {
        return Err(MeshDispError::Empty);
    }
    let frame = match forced {
        Some(f) => f,
        None => {
            let (axis, flip) = match opts.axis {
                AxisChoice::Z => (2, opts.flip),
                AxisChoice::X => (0, opts.flip),
                AxisChoice::Y => (1, opts.flip),
                AxisChoice::Best => {
                    let (a, f) = best_axis(&scaled);
                    (a, f ^ opts.flip)
                }
            };
            Frame::new(axis, flip)
        }
    };
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for t in &scaled {
        for p in t {
            let l = frame.local(*p);
            lo = lo.min(l);
            hi = hi.max(l);
        }
    }
    let target = frame.local(opts.position);
    let shift = match opts.origin {
        Origin::Keep => DVec3::ZERO,
        Origin::MinCorner => target - lo,
        Origin::Center => DVec3::new(target.x - (lo.x + hi.x) * 0.5, target.y - (lo.y + hi.y) * 0.5, target.z - lo.z),
    };
    let tris: Vec<LocalTri> = scaled
        .iter()
        .filter_map(|t| {
            let moved = t.map(|p| frame.world(frame.local(p) + shift));
            LocalTri::new(&moved, &frame)
        })
        .collect();
    if tris.is_empty() {
        return Err(MeshDispError::Empty);
    }
    Ok(Prepared { frame, tris, lo: lo + shift, hi: hi + shift })
}

fn median_edge(tris: &[LocalTri]) -> f64 {
    let step = (tris.len() / 4096).max(1);
    let mut edges: Vec<f64> = Vec::new();
    for t in tris.iter().step_by(step) {
        for k in 0..3 {
            let (a, b) = (t.p[k], t.p[(k + 1) % 3]);
            let d = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
            if d > 1e-6 {
                edges.push(d);
            }
        }
    }
    if edges.is_empty() {
        return f64::INFINITY;
    }
    edges.sort_by(|a, b| a.total_cmp(b));
    edges[edges.len() / 2]
}

pub fn auto_power(tile: f64, edge: f64) -> u32 {
    (2..=4).find(|p| tile / f64::from(1u32 << p) <= edge).unwrap_or(4)
}

pub fn check_material(material: &str) -> Result<(), MeshDispError> {
    if material.trim().is_empty() || material.len() > 255 || material.contains(['"', '\n', '\r', '\0']) {
        return Err(MeshDispError::Option("invalid material name".to_owned()));
    }
    Ok(())
}

pub fn check_options(opts: &MeshDispOptions) -> Result<(), MeshDispError> {
    let bad = |m: &str| Err(MeshDispError::Option(m.to_owned()));
    if !opts.tile_size.is_finite() || !(MIN_TILE..=MAX_TILE).contains(&opts.tile_size) {
        return bad("tile size must be between 16 and 4096");
    }
    if opts.smooth > MAX_SMOOTH {
        return bad("smoothing is at most 64 iterations");
    }
    if opts.margin > MAX_MARGIN {
        return bad("margin is at most 8 tiles");
    }
    if !opts.thickness.is_finite() || !(1.0..=4096.0).contains(&opts.thickness) {
        return bad("thickness must be between 1 and 4096");
    }
    if !opts.uv_scale.is_finite() || !(0.001..=64.0).contains(&opts.uv_scale) {
        return bad("texture scale must be between 0.001 and 64");
    }
    if !opts.position.is_finite() || opts.position.abs().max_element() > 131_072.0 {
        return bad("position is out of range");
    }
    if opts.base_height.is_some_and(|b| !b.is_finite() || b.abs() > 131_072.0) {
        return bad("base height is out of range");
    }
    if let PowerChoice::Fixed(p) = opts.power
        && !(2..=4).contains(&p)
    {
        return bad("power must be 2, 3, 4 or auto");
    }
    check_material(&opts.material)
}

struct Lattice {
    nx: usize,
    ny: usize,
    cells: usize,
    w: usize,
    h: usize,
}

fn tile_flags(candidate: &[bool], hit: &[bool], l: &Lattice) -> Vec<bool> {
    let mut kept = vec![false; l.nx * l.ny];
    for tj in 0..l.ny {
        for ti in 0..l.nx {
            if !candidate[tj * l.nx + ti] {
                continue;
            }
            let (i0, j0) = (ti * l.cells, tj * l.cells);
            let mut ok = false;
            for dj in 0..l.cells {
                for di in 0..l.cells {
                    let k = (j0 + dj) * l.w + i0 + di;
                    let corners = [k, k + 1, k + l.w, k + l.w + 1];
                    if corners.iter().filter(|q| hit[**q]).count() >= 3 {
                        ok = true;
                    }
                    if di > 0 && dj > 0 && hit[k] {
                        ok = true;
                    }
                }
            }
            kept[tj * l.nx + ti] = ok;
        }
    }
    kept
}

fn grow(kept: &mut [bool], l: &Lattice, margin: usize) {
    let source = kept.to_vec();
    for tj in 0..l.ny {
        for ti in 0..l.nx {
            if source[tj * l.nx + ti] {
                continue;
            }
            let near = (tj.saturating_sub(margin)..=(tj + margin).min(l.ny - 1))
                .any(|j| (ti.saturating_sub(margin)..=(ti + margin).min(l.nx - 1)).any(|i| source[j * l.nx + i]));
            kept[tj * l.nx + ti] = near;
        }
    }
}

fn fill_nearest(value: &mut [f64], hit: &[bool], valid: &[bool], l: &Lattice) -> usize {
    let mut queue: std::collections::VecDeque<usize> = (0..l.w * l.h).filter(|k| hit[*k]).collect();
    let mut set = hit.to_vec();
    let mut filled = 0;
    while let Some(k) = queue.pop_front() {
        let (gi, gj) = ((k % l.w) as isize, (k / l.w) as isize);
        for dj in -1..=1isize {
            for di in -1..=1isize {
                let (ni, nj) = (gi + di, gj + dj);
                if ni < 0 || nj < 0 || ni >= l.w as isize || nj >= l.h as isize {
                    continue;
                }
                let n = nj as usize * l.w + ni as usize;
                if valid[n] && !set[n] {
                    set[n] = true;
                    value[n] = value[k];
                    filled += 1;
                    queue.push_back(n);
                }
            }
        }
    }
    filled
}

fn smooth_nodes(value: &mut [f64], valid: &[bool], l: &Lattice, iterations: u32) {
    for _ in 0..iterations {
        let old = value.to_vec();
        for gj in 0..l.h {
            for gi in 0..l.w {
                let k = gj * l.w + gi;
                if !valid[k] {
                    continue;
                }
                let mut sum = 0.0;
                let mut n = 0.0;
                for (di, dj) in [(-1isize, 0isize), (1, 0), (0, -1), (0, 1)] {
                    let (ni, nj) = (gi as isize + di, gj as isize + dj);
                    if ni < 0 || nj < 0 || ni >= l.w as isize || nj >= l.h as isize {
                        continue;
                    }
                    let q = nj as usize * l.w + ni as usize;
                    if valid[q] {
                        sum += old[q];
                        n += 1.0;
                    }
                }
                if n > 0.0 {
                    value[k] = old[k] * 0.5 + sum / n * 0.5;
                }
            }
        }
    }
}

pub fn plan(world: &[Tri], opts: &MeshDispOptions) -> Result<TilePlan, MeshDispError> {
    check_options(opts)?;
    let prep = prepare(world, opts, None)?;
    let frame = prep.frame;
    let tile = opts.tile_size;
    let power = match opts.power {
        PowerChoice::Fixed(p) => p,
        PowerChoice::Auto => auto_power(tile, median_edge(&prep.tris)),
    };
    let cells = 1usize << power;
    let spacing = tile / cells as f64;
    let margin = opts.margin as usize;
    let lo_u = (prep.lo.x / tile + SNAP).floor() * tile - margin as f64 * tile;
    let lo_v = (prep.lo.y / tile + SNAP).floor() * tile - margin as f64 * tile;
    let span = |lo: f64, hi: f64| ((hi - lo) / tile - SNAP).ceil().max(1.0) as usize + 2 * margin;
    let nx = span(lo_u + margin as f64 * tile, prep.hi.x);
    let ny = span(lo_v + margin as f64 * tile, prep.hi.y);
    if nx.saturating_mul(ny) > MAX_TILES {
        return Err(MeshDispError::TooManyTiles(nx.saturating_mul(ny), MAX_TILES));
    }
    let lat = Lattice { nx, ny, cells, w: nx * cells + 1, h: ny * cells + 1 };
    if lat.w.saturating_mul(lat.h) > MAX_NODES {
        return Err(MeshDispError::TooManyTiles(nx * ny, MAX_NODES / (cells * cells)));
    }
    let (w, h) = (lat.w, lat.h);
    let sampler = Sampler::new(prep.tris.clone(), spacing);
    let mut candidate = vec![false; nx * ny];
    for t in &prep.tris {
        let (u0, u1, v0, v1) = t.bounds();
        let i0 = (((u0 - lo_u) / tile + SNAP).floor().max(0.0) as usize).min(nx - 1);
        let i1 = (((u1 - lo_u) / tile - SNAP).ceil().max(1.0) as usize - 1).clamp(i0, nx - 1);
        let j0 = (((v0 - lo_v) / tile + SNAP).floor().max(0.0) as usize).min(ny - 1);
        let j1 = (((v1 - lo_v) / tile - SNAP).ceil().max(1.0) as usize - 1).clamp(j0, ny - 1);
        for j in j0..=j1 {
            for i in i0..=i1 {
                candidate[j * nx + i] = true;
            }
        }
    }
    let mut value = vec![f64::NAN; w * h];
    let mut done = vec![false; w * h];
    let mut hit = vec![false; w * h];
    let mut over = vec![false; w * h];
    let mut scratch = Vec::with_capacity(16);
    for tj in 0..ny {
        for ti in 0..nx {
            if !candidate[tj * nx + ti] {
                continue;
            }
            for gj in tj * cells..=(tj + 1) * cells {
                for gi in ti * cells..=(ti + 1) * cells {
                    let k = gj * w + gi;
                    if done[k] {
                        continue;
                    }
                    done[k] = true;
                    let (u, v) = (lo_u + gi as f64 * spacing, lo_v + gj as f64 * spacing);
                    if let Some(s) = sampler.sample(u, v, opts.pick, &mut scratch) {
                        value[k] = s.height;
                        hit[k] = true;
                        over[k] = s.overhang;
                    }
                }
            }
        }
    }
    let mut kept = tile_flags(&candidate, &hit, &lat);
    if !kept.iter().any(|k| *k) {
        return Err(MeshDispError::NoHit);
    }
    if margin > 0 {
        grow(&mut kept, &lat, margin);
    }
    let mut valid = vec![false; w * h];
    for tj in 0..ny {
        for ti in 0..nx {
            if kept[tj * nx + ti] {
                for gj in tj * cells..=(tj + 1) * cells {
                    for gi in ti * cells..=(ti + 1) * cells {
                        valid[gj * w + gi] = true;
                    }
                }
            }
        }
    }
    let mut overhang_nodes = 0;
    let mut lowest = f64::INFINITY;
    for k in 0..w * h {
        if valid[k] && hit[k] {
            lowest = lowest.min(value[k]);
            if over[k] {
                overhang_nodes += 1;
            }
        } else {
            hit[k] = false;
        }
    }
    let mut filled_nodes = 0;
    match opts.fill {
        Fill::Base => {
            for k in 0..w * h {
                if valid[k] && !hit[k] {
                    value[k] = lowest;
                    filled_nodes += 1;
                }
            }
        }
        Fill::Nearest => filled_nodes = fill_nearest(&mut value, &hit, &valid, &lat),
    }
    smooth_nodes(&mut value, &valid, &lat, opts.smooth);
    let lowest = (0..w * h).filter(|k| valid[*k]).map(|k| value[k]).fold(f64::INFINITY, f64::min);
    let plane = lowest.floor();
    let base = opts.base_height.unwrap_or(plane - opts.thickness);
    if base > plane - 1.0 {
        return Err(MeshDispError::BadBase(base, plane));
    }
    let skipped = candidate.iter().zip(&kept).filter(|(c, k)| **c && !**k).count();
    let mut plan = TilePlan {
        frame,
        power,
        tile,
        spacing,
        lo_u,
        lo_v,
        nx,
        ny,
        kept,
        heights: value,
        plane,
        base,
        thickness: opts.thickness,
        skipped,
        warnings: Warnings { overhang_nodes, filled_nodes, over_limit: false },
    };
    let solids = plan.tile_count() + if opts.skirt { plan.edge_count() } else { 0 };
    plan.warnings.over_limit = solids > DISP_LIMIT;
    Ok(plan)
}
