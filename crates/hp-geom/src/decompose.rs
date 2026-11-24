use glam::{DVec3, IVec3};

use crate::convex_hull;

pub const MAX_COLLISION_PIECES: usize = 40;
pub const MIN_HULL_VERTICES: usize = 8;
pub const MAX_HULL_VERTICES: usize = 128;
pub const FLAT_THICKNESS: f64 = 1.0;

const MIN_RESOLUTION: usize = 8;
const MAX_RESOLUTION: usize = 64;
const CANDIDATES_PER_AXIS: i32 = 7;
const MAX_TRIANGLES: usize = 200_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecomposeOptions {
    pub max_pieces: usize,
    pub max_hull_vertices: usize,
    pub resolution: usize,
    pub concavity: f64,
}

impl Default for DecomposeOptions {
    fn default() -> Self {
        Self { max_pieces: 16, max_hull_vertices: 32, resolution: 40, concavity: 0.02 }
    }
}

impl DecomposeOptions {
    pub fn clamped(self) -> Self {
        Self {
            max_pieces: self.max_pieces.clamp(1, MAX_COLLISION_PIECES),
            max_hull_vertices: self.max_hull_vertices.clamp(MIN_HULL_VERTICES, MAX_HULL_VERTICES),
            resolution: self.resolution.clamp(MIN_RESOLUTION, MAX_RESOLUTION),
            concavity: if self.concavity.is_finite() { self.concavity.clamp(0.0, 1.0) } else { 0.02 },
        }
    }
}

fn outside_distance(hull: &crate::ConvexHull, p: DVec3) -> f64 {
    hull.faces.iter().map(|f| f.plane.normal.dot(p) - f.plane.dist).fold(f64::NEG_INFINITY, f64::max)
}

fn thicken(points: &[DVec3]) -> Vec<DVec3> {
    let Some(&first) = points.first() else { return Vec::new() };
    let far = points.iter().copied().max_by(|a, b| a.distance_squared(first).total_cmp(&b.distance_squared(first)));
    let Some(far) = far.filter(|f| f.distance_squared(first) > 1e-12) else {
        return points.iter().flat_map(|p| [*p - DVec3::splat(0.5), *p + DVec3::splat(0.5)]).collect();
    };
    let axis = (far - first).normalize();
    let third = points
        .iter()
        .copied()
        .max_by(|a, b| (*a - first).cross(axis).length_squared().total_cmp(&(*b - first).cross(axis).length_squared()));
    let normal = third
        .map(|t| axis.cross(t - first))
        .filter(|n| n.length_squared() > 1e-12)
        .map(DVec3::normalize)
        .unwrap_or_else(|| axis.any_orthonormal_vector());
    let side = normal.cross(axis).normalize_or_zero();
    let thin_side = points.iter().all(|p| (*p - first).dot(side).abs() < 1e-6);
    let half = FLAT_THICKNESS * 0.5;
    let mut out = Vec::with_capacity(points.len() * 4);
    for &p in points {
        out.push(p + normal * half);
        out.push(p - normal * half);
        if thin_side {
            out.push(p + side * half);
            out.push(p - side * half);
        }
    }
    out
}

pub fn limited_hull(points: &[DVec3], max_vertices: usize) -> Option<Vec<DVec3>> {
    let finite: Vec<DVec3> = points.iter().copied().filter(|p| p.is_finite()).collect();
    let max_vertices = max_vertices.max(4);
    let hull = match convex_hull(&finite) {
        Ok(hull) => hull,
        Err(_) => convex_hull(&thicken(&finite)).ok()?,
    };
    if hull.points.len() <= max_vertices {
        return Some(hull.points);
    }
    let candidates = hull.points;
    let a = (0..candidates.len()).min_by(|&i, &j| candidates[i].x.total_cmp(&candidates[j].x))?;
    let b = (0..candidates.len()).max_by(|&i, &j| {
        candidates[i].distance_squared(candidates[a]).total_cmp(&candidates[j].distance_squared(candidates[a]))
    })?;
    let line = (candidates[b] - candidates[a]).normalize_or_zero();
    let c = (0..candidates.len()).max_by(|&i, &j| {
        let di = (candidates[i] - candidates[a]).cross(line).length_squared();
        let dj = (candidates[j] - candidates[a]).cross(line).length_squared();
        di.total_cmp(&dj)
    })?;
    let normal = (candidates[b] - candidates[a]).cross(candidates[c] - candidates[a]).normalize_or_zero();
    let d = (0..candidates.len()).max_by(|&i, &j| {
        let di = (candidates[i] - candidates[a]).dot(normal).abs();
        let dj = (candidates[j] - candidates[a]).dot(normal).abs();
        di.total_cmp(&dj)
    })?;
    let mut chosen = vec![a, b, c, d];
    chosen.sort_unstable();
    chosen.dedup();
    let mut current = convex_hull(&chosen.iter().map(|&i| candidates[i]).collect::<Vec<_>>()).ok()?;
    while current.points.len() < max_vertices {
        let next = (0..candidates.len())
            .filter(|i| !chosen.contains(i))
            .map(|i| (i, outside_distance(&current, candidates[i])))
            .max_by(|x, y| x.1.total_cmp(&y.1));
        let Some((index, distance)) = next else { break };
        if distance <= 1e-6 {
            break;
        }
        chosen.push(index);
        let trial = convex_hull(&chosen.iter().map(|&i| candidates[i]).collect::<Vec<_>>()).ok()?;
        if trial.points.len() > max_vertices {
            chosen.pop();
            break;
        }
        current = trial;
    }
    Some(current.points)
}

struct Grid {
    origin: DVec3,
    cell: f64,
    dims: IVec3,
    solid: Vec<bool>,
}

impl Grid {
    fn index(&self, v: IVec3) -> usize {
        (v.x + self.dims.x * (v.y + self.dims.y * v.z)) as usize
    }

    fn inside(&self, v: IVec3) -> bool {
        v.cmpge(IVec3::ZERO).all() && v.cmplt(self.dims).all()
    }

    fn is_solid(&self, v: IVec3) -> bool {
        self.inside(v) && self.solid[self.index(v)]
    }

    fn world(&self, v: IVec3) -> DVec3 {
        self.origin + v.as_dvec3() * self.cell
    }
}

fn axis_overlap(axis: DVec3, tri: &[DVec3; 3], half: DVec3) -> bool {
    if axis.length_squared() < 1e-18 {
        return true;
    }
    let p = tri.map(|v| v.dot(axis));
    let r = half.x * axis.x.abs() + half.y * axis.y.abs() + half.z * axis.z.abs();
    let min = p[0].min(p[1]).min(p[2]);
    let max = p[0].max(p[1]).max(p[2]);
    !(min > r || max < -r)
}

fn triangle_box(tri: &[DVec3; 3], center: DVec3, half: DVec3) -> bool {
    let v = tri.map(|p| p - center);
    let edges = [v[1] - v[0], v[2] - v[1], v[0] - v[2]];
    for axis in [DVec3::X, DVec3::Y, DVec3::Z] {
        if !axis_overlap(axis, &v, half) {
            return false;
        }
        for edge in edges {
            if !axis_overlap(axis.cross(edge), &v, half) {
                return false;
            }
        }
    }
    axis_overlap(edges[0].cross(edges[1]), &v, half)
}

fn voxelize(triangles: &[[DVec3; 3]], resolution: usize) -> Option<Grid> {
    let mut lo = DVec3::splat(f64::INFINITY);
    let mut hi = DVec3::splat(f64::NEG_INFINITY);
    for p in triangles.iter().flatten() {
        lo = lo.min(*p);
        hi = hi.max(*p);
    }
    let extent = (hi - lo).max_element();
    if !extent.is_finite() || extent <= 1e-9 {
        return None;
    }
    let cell = extent / resolution as f64;
    let pad = DVec3::splat(cell * 0.01);
    let lo = lo - pad;
    let size = hi + pad - lo;
    let dims = IVec3::new(
        ((size.x / cell).ceil() as i32).clamp(1, resolution as i32 + 1),
        ((size.y / cell).ceil() as i32).clamp(1, resolution as i32 + 1),
        ((size.z / cell).ceil() as i32).clamp(1, resolution as i32 + 1),
    );
    let mut grid = Grid { origin: lo, cell, dims, solid: vec![false; (dims.x * dims.y * dims.z) as usize] };
    let half = DVec3::splat(cell * 0.5 + cell * 1e-6);
    let mut surface = vec![false; grid.solid.len()];
    for tri in triangles {
        let tlo = tri[0].min(tri[1]).min(tri[2]);
        let thi = tri[0].max(tri[1]).max(tri[2]);
        let a = ((tlo - lo) / cell).floor().as_ivec3().max(IVec3::ZERO);
        let b = ((thi - lo) / cell).floor().as_ivec3().min(dims - IVec3::ONE);
        for z in a.z..=b.z {
            for y in a.y..=b.y {
                for x in a.x..=b.x {
                    let v = IVec3::new(x, y, z);
                    let index = grid.index(v);
                    if surface[index] {
                        continue;
                    }
                    let center = grid.world(v) + DVec3::splat(cell * 0.5);
                    if triangle_box(tri, center, half) {
                        surface[index] = true;
                    }
                }
            }
        }
    }
    let mut outside = vec![false; grid.solid.len()];
    let mut stack: Vec<IVec3> = Vec::new();
    for z in 0..dims.z {
        for y in 0..dims.y {
            for x in 0..dims.x {
                let v = IVec3::new(x, y, z);
                let border = x == 0 || y == 0 || z == 0 || x == dims.x - 1 || y == dims.y - 1 || z == dims.z - 1;
                let index = grid.index(v);
                if border && !surface[index] && !outside[index] {
                    outside[index] = true;
                    stack.push(v);
                }
            }
        }
    }
    let steps = [IVec3::X, IVec3::NEG_X, IVec3::Y, IVec3::NEG_Y, IVec3::Z, IVec3::NEG_Z];
    while let Some(v) = stack.pop() {
        for step in steps {
            let n = v + step;
            if grid.inside(n) {
                let index = grid.index(n);
                if !surface[index] && !outside[index] {
                    outside[index] = true;
                    stack.push(n);
                }
            }
        }
    }
    for (slot, out) in grid.solid.iter_mut().zip(&outside) {
        *slot = !out;
    }
    Some(grid)
}

#[derive(Clone, Copy, Debug)]
struct Region {
    lo: IVec3,
    hi: IVec3,
    count: usize,
    hull: f64,
    done: bool,
}

impl Region {
    fn concavity(&self) -> f64 {
        (self.hull - self.count as f64).max(0.0)
    }
}

fn tight(grid: &Grid, lo: IVec3, hi: IVec3) -> Option<(IVec3, IVec3, usize)> {
    let mut a = IVec3::splat(i32::MAX);
    let mut b = IVec3::splat(i32::MIN);
    let mut count = 0;
    for z in lo.z..hi.z {
        for y in lo.y..hi.y {
            for x in lo.x..hi.x {
                let v = IVec3::new(x, y, z);
                if grid.solid[grid.index(v)] {
                    a = a.min(v);
                    b = b.max(v);
                    count += 1;
                }
            }
        }
    }
    (count > 0).then_some((a, b + IVec3::ONE, count))
}

fn region_points(grid: &Grid, lo: IVec3, hi: IVec3) -> Vec<DVec3> {
    let w = (hi.x - lo.x + 1) as usize;
    let h = (hi.y - lo.y + 1) as usize;
    let mut column: Vec<(i32, i32)> = vec![(i32::MAX, i32::MIN); w * h];
    for z in lo.z..hi.z {
        for y in lo.y..hi.y {
            for x in lo.x..hi.x {
                let v = IVec3::new(x, y, z);
                if !grid.solid[grid.index(v)] {
                    continue;
                }
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let slot = &mut column[((y - lo.y + dy) as usize) * w + (x - lo.x + dx) as usize];
                    slot.0 = slot.0.min(z);
                    slot.1 = slot.1.max(z + 1);
                }
            }
        }
    }
    let mut points = Vec::new();
    for (index, (zmin, zmax)) in column.into_iter().enumerate() {
        if zmin > zmax {
            continue;
        }
        let x = lo.x as f64 + (index % w) as f64;
        let y = lo.y as f64 + (index / w) as f64;
        points.push(DVec3::new(x, y, zmin as f64));
        points.push(DVec3::new(x, y, zmax as f64));
    }
    points
}

fn region(grid: &Grid, lo: IVec3, hi: IVec3) -> Option<Region> {
    let (lo, hi, count) = tight(grid, lo, hi)?;
    let hull = convex_hull(&region_points(grid, lo, hi)).map(|h| h.volume()).unwrap_or(count as f64);
    Some(Region { lo, hi, count, hull: hull.max(count as f64), done: false })
}

fn split(grid: &Grid, leaf: &Region) -> Option<(Region, Region)> {
    let mut best: Option<(f64, Region, Region)> = None;
    for axis in 0..3 {
        let span = leaf.hi[axis] - leaf.lo[axis];
        if span < 2 {
            continue;
        }
        let steps = CANDIDATES_PER_AXIS.min(span - 1);
        for step in 1..=steps {
            let cut = leaf.lo[axis] + (span * step) / (steps + 1);
            if cut <= leaf.lo[axis] || cut >= leaf.hi[axis] {
                continue;
            }
            let mut a_hi = leaf.hi;
            a_hi[axis] = cut;
            let mut b_lo = leaf.lo;
            b_lo[axis] = cut;
            let (Some(a), Some(b)) = (region(grid, leaf.lo, a_hi), region(grid, b_lo, leaf.hi)) else {
                continue;
            };
            let balance = (a.count.min(b.count) as f64 / leaf.count as f64).min(0.5);
            let score = a.hull + b.hull + leaf.hull * 0.002 * (0.5 - balance);
            if best.as_ref().is_none_or(|(s, _, _)| score < *s) {
                best = Some((score, a, b));
            }
        }
    }
    let (score, a, b) = best?;
    (score < leaf.hull * 0.995).then_some((a, b))
}

fn clip(polygon: &mut Vec<DVec3>, axis: usize, value: f64, keep_above: bool) {
    let mut out = Vec::with_capacity(polygon.len() + 2);
    let side = |p: DVec3| if keep_above { p[axis] - value } else { value - p[axis] };
    for i in 0..polygon.len() {
        let a = polygon[i];
        let b = polygon[(i + 1) % polygon.len()];
        let (da, db) = (side(a), side(b));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            let t = da / (da - db);
            out.push(a + (b - a) * t);
        }
    }
    *polygon = out;
}

fn leaf_points(triangles: &[[DVec3; 3]], grid: &Grid, leaf: &Region) -> Vec<DVec3> {
    let eps = grid.cell * 1e-4;
    let lo = grid.world(leaf.lo) - DVec3::splat(eps);
    let hi = grid.world(leaf.hi) + DVec3::splat(eps);
    let mut points = Vec::new();
    for tri in triangles {
        let tlo = tri[0].min(tri[1]).min(tri[2]);
        let thi = tri[0].max(tri[1]).max(tri[2]);
        if tlo.cmpgt(hi).any() || thi.cmplt(lo).any() {
            continue;
        }
        if tlo.cmpge(lo).all() && thi.cmple(hi).all() {
            points.extend_from_slice(tri);
            continue;
        }
        let mut polygon = tri.to_vec();
        for axis in 0..3 {
            clip(&mut polygon, axis, lo[axis], true);
            clip(&mut polygon, axis, hi[axis], false);
            if polygon.is_empty() {
                break;
            }
        }
        points.extend(polygon);
    }
    for corner in 0..8 {
        let v = IVec3::new(
            if corner & 1 == 0 { leaf.lo.x } else { leaf.hi.x },
            if corner & 2 == 0 { leaf.lo.y } else { leaf.hi.y },
            if corner & 4 == 0 { leaf.lo.z } else { leaf.hi.z },
        );
        let buried = (0..8).all(|n| {
            let o = IVec3::new(-(n & 1), -((n >> 1) & 1), -((n >> 2) & 1));
            grid.is_solid(v + o)
        });
        if buried {
            points.push(grid.world(v));
        }
    }
    points
}

fn clean(triangles: &[[DVec3; 3]]) -> Vec<[DVec3; 3]> {
    triangles
        .iter()
        .filter(|t| t.iter().all(|p| p.is_finite()))
        .filter(|t| (t[1] - t[0]).cross(t[2] - t[0]).length_squared() > 1e-18)
        .take(MAX_TRIANGLES)
        .copied()
        .collect()
}

pub fn decompose(triangles: &[[DVec3; 3]], options: DecomposeOptions) -> Vec<Vec<DVec3>> {
    let options = options.clamped();
    let triangles = clean(triangles);
    if triangles.is_empty() {
        return Vec::new();
    }
    let all: Vec<DVec3> = triangles.iter().flatten().copied().collect();
    let whole = || limited_hull(&all, options.max_hull_vertices).into_iter().collect::<Vec<_>>();
    if options.max_pieces <= 1 {
        return whole();
    }
    let Some(grid) = voxelize(&triangles, options.resolution) else { return whole() };
    let Some(root) = region(&grid, IVec3::ZERO, grid.dims) else { return whole() };
    let total = (root.count as f64).max(1.0);
    let mut leaves = vec![root];
    while leaves.len() < options.max_pieces {
        let pick = leaves
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.done)
            .max_by(|a, b| a.1.concavity().total_cmp(&b.1.concavity()))
            .map(|(i, l)| (i, l.concavity()));
        let Some((index, concavity)) = pick else { break };
        if concavity / total <= options.concavity {
            break;
        }
        match split(&grid, &leaves[index]) {
            Some((a, b)) => {
                leaves[index] = a;
                leaves.push(b);
            }
            None => leaves[index].done = true,
        }
    }
    if leaves.len() == 1 {
        return whole();
    }
    let mut pieces: Vec<Vec<DVec3>> = leaves
        .iter()
        .filter_map(|leaf| {
            let points = leaf_points(&triangles, &grid, leaf);
            if points.len() >= 3 {
                limited_hull(&points, options.max_hull_vertices)
            } else {
                let lo = grid.world(leaf.lo);
                let hi = grid.world(leaf.hi);
                let corners: Vec<DVec3> = (0..8)
                    .map(|i| {
                        DVec3::new(
                            if i & 1 == 0 { lo.x } else { hi.x },
                            if i & 2 == 0 { lo.y } else { hi.y },
                            if i & 4 == 0 { lo.z } else { hi.z },
                        )
                    })
                    .collect();
                limited_hull(&corners, options.max_hull_vertices)
            }
        })
        .collect();
    if pieces.is_empty() {
        pieces = whole();
    }
    pieces
}

pub fn hull_triangles(points: &[DVec3]) -> Vec<[DVec3; 3]> {
    let Ok(hull) = convex_hull(points) else { return Vec::new() };
    let mut out = Vec::new();
    for face in &hull.faces {
        let polygon = hull.face_polygon(face);
        for i in 1..polygon.len().saturating_sub(1) {
            out.push([polygon[0], polygon[i], polygon[i + 1]]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cuboid(lo: DVec3, hi: DVec3) -> Vec<[DVec3; 3]> {
        let c = |i: usize| {
            DVec3::new(
                if i & 1 == 0 { lo.x } else { hi.x },
                if i & 2 == 0 { lo.y } else { hi.y },
                if i & 4 == 0 { lo.z } else { hi.z },
            )
        };
        let quads = [[0, 2, 6, 4], [1, 5, 7, 3], [0, 4, 5, 1], [2, 3, 7, 6], [0, 1, 3, 2], [4, 6, 7, 5]];
        quads.iter().flat_map(|q| [[c(q[0]), c(q[1]), c(q[2])], [c(q[0]), c(q[2]), c(q[3])]]).collect()
    }

    fn sphere(rings: usize) -> Vec<[DVec3; 3]> {
        let point = |i: usize, j: usize| {
            let theta = std::f64::consts::PI * i as f64 / rings as f64;
            let phi = std::f64::consts::TAU * j as f64 / (rings * 2) as f64;
            DVec3::new(theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos()) * 50.0
        };
        let mut out = Vec::new();
        for i in 0..rings {
            for j in 0..rings * 2 {
                let (a, b, c, d) = (point(i, j), point(i + 1, j), point(i + 1, j + 1), point(i, j + 1));
                out.push([a, b, c]);
                out.push([a, c, d]);
            }
        }
        out
    }

    fn contains(pieces: &[Vec<DVec3>], p: DVec3, tolerance: f64) -> bool {
        pieces.iter().any(|piece| {
            let hull = convex_hull(piece).unwrap();
            outside_distance(&hull, p) <= tolerance
        })
    }

    #[test]
    fn a_limited_hull_keeps_the_vertex_budget_and_stays_inside_the_full_hull() {
        let points: Vec<DVec3> = sphere(16).into_iter().flatten().collect();
        let full = convex_hull(&points).unwrap();
        assert!(full.points.len() > 64);
        for budget in [8, 16, 32, 64] {
            let limited = limited_hull(&points, budget).unwrap();
            assert!(limited.len() <= budget, "{budget}: {}", limited.len());
            assert!(limited.len() >= 4);
            assert!(limited.iter().all(|p| outside_distance(&full, *p) < 1e-6));
            let hull = convex_hull(&limited).unwrap();
            let floor = match budget {
                8 => 0.3,
                16 => 0.55,
                32 => 0.72,
                _ => 0.85,
            };
            assert!(hull.volume() > full.volume() * floor, "{budget}");
        }
        let flat = [DVec3::ZERO, DVec3::X * 10.0, DVec3::Y * 10.0, DVec3::new(10.0, 10.0, 0.0)];
        let thick = limited_hull(&flat, 32).unwrap();
        let hull = convex_hull(&thick).unwrap();
        assert!((hull.volume() - 100.0 * FLAT_THICKNESS).abs() < 1e-6, "a flat quad gets a thickness");
    }

    #[test]
    fn a_box_stays_one_piece_and_an_l_shape_splits_into_convex_parts() {
        let boxed = decompose(&cuboid(DVec3::ZERO, DVec3::new(64.0, 32.0, 16.0)), DecomposeOptions::default());
        assert_eq!(boxed.len(), 1);
        assert_eq!(boxed[0].len(), 8);
        let mut l_shape = cuboid(DVec3::ZERO, DVec3::new(100.0, 20.0, 20.0));
        l_shape.extend(cuboid(DVec3::ZERO, DVec3::new(20.0, 100.0, 20.0)));
        let pieces = decompose(&l_shape, DecomposeOptions::default());
        assert!((2..=4).contains(&pieces.len()), "{}", pieces.len());
        let volume: f64 = pieces.iter().map(|p| convex_hull(p).unwrap().volume()).sum();
        let solid = 100.0 * 20.0 * 20.0 * 2.0 - 20.0 * 20.0 * 20.0;
        assert!(volume < solid * 1.25, "{volume} vs {solid}");
        assert!(volume > solid * 0.8, "{volume} vs {solid}");
        for p in [DVec3::new(95.0, 10.0, 10.0), DVec3::new(10.0, 95.0, 10.0), DVec3::new(10.0, 10.0, 10.0)] {
            assert!(contains(&pieces, p, 0.5), "{p:?}");
        }
        assert!(!contains(&pieces, DVec3::new(80.0, 80.0, 10.0), 0.0), "the inside corner stays empty");
    }

    #[test]
    fn decomposition_respects_the_piece_and_vertex_limits() {
        let mut ring = Vec::new();
        for i in 0..12 {
            let angle = std::f64::consts::TAU * i as f64 / 12.0;
            let c = DVec3::new(angle.cos(), angle.sin(), 0.0) * 200.0;
            ring.extend(cuboid(c - DVec3::splat(30.0), c + DVec3::splat(30.0)));
        }
        ring.extend(sphere(12).into_iter().map(|t| t.map(|p| p + DVec3::new(0.0, 0.0, 120.0))));
        for (max_pieces, max_hull_vertices) in [(1, 8), (4, 16), (16, 32), (40, 128), (500, 500)] {
            let options = DecomposeOptions { max_pieces, max_hull_vertices, ..DecomposeOptions::default() };
            let pieces = decompose(&ring, options);
            let clamped = options.clamped();
            assert!(!pieces.is_empty());
            assert!(pieces.len() <= clamped.max_pieces, "{max_pieces}: {}", pieces.len());
            for piece in &pieces {
                assert!(piece.len() >= 4 && piece.len() <= clamped.max_hull_vertices);
                let hull = convex_hull(piece).unwrap();
                assert!(hull.volume() > 0.0);
                assert!(hull_triangles(piece).len() <= 2 * clamped.max_hull_vertices);
            }
        }
        let many = decompose(&ring, DecomposeOptions { max_pieces: 40, ..DecomposeOptions::default() });
        assert!(many.len() >= 12, "separate blocks get their own pieces: {}", many.len());
        assert!(decompose(&[], DecomposeOptions::default()).is_empty());
        let nan = [[DVec3::NAN, DVec3::X, DVec3::Y]];
        assert!(decompose(&nan, DecomposeOptions::default()).is_empty());
    }

    #[test]
    fn a_sphere_stays_a_few_pieces() {
        for rings in [8, 16, 32] {
            let pieces = decompose(&sphere(rings), DecomposeOptions::default());
            assert!(pieces.len() <= 2, "{rings}: {}", pieces.len());
        }
    }

    #[test]
    fn an_open_flat_mesh_still_gets_a_thin_solid_piece() {
        let quad = [
            [DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0), DVec3::new(64.0, 64.0, 0.0)],
            [DVec3::ZERO, DVec3::new(64.0, 64.0, 0.0), DVec3::new(0.0, 64.0, 0.0)],
        ];
        let pieces = decompose(&quad, DecomposeOptions::default());
        assert!(!pieces.is_empty());
        for piece in &pieces {
            assert!(convex_hull(piece).unwrap().volume() > 0.0);
        }
    }
}
