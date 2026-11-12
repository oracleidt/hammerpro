use glam::DVec3;
use hp_vmf::multiblend::{MultiBlend, remove_multiblend};
use hp_vmf::{DispInfo, MAX_DISP_POWER, Side};

use crate::csg::{ClipPiece, clip_geom};
use crate::disp::{DISP_TAG_BUILDABLE, DISP_TAG_WALKABLE, flat_grid, grid_indices, source_corners, update_disp_tags};
use crate::util::polygon_area_vector;
use crate::{Plane, SolidGeom, build_solid};

pub const MIN_CUT_DISP_POWER: u32 = 2;
const MAX_STEPS: usize = 256;
const GRID_TOL: f64 = 1e-3;
const EDGE_TOL: f64 = 0.01;
const CHAMFER: f64 = 1.0;
const MIN_CHAMFER: f64 = 0.25;

struct Source<'a> {
    corners: [DVec3; 4],
    disp: &'a DispInfo,
    n: usize,
    vectors: Vec<DVec3>,
    blend: Option<MultiBlend>,
}

impl<'a> Source<'a> {
    fn new(face: &[DVec3], disp: &'a DispInfo) -> Option<Self> {
        if !(MIN_CUT_DISP_POWER..=MAX_DISP_POWER).contains(&disp.power) {
            return None;
        }
        let n = disp.row_len();
        let count = n * n;
        if disp.normals.len() != count
            || disp.distances.len() != count
            || disp.offsets.len() != count
            || disp.alphas.len() != count
        {
            return None;
        }
        let (corners, _) = source_corners(face, disp.start_position).ok()?;
        let vectors = disp.normals.iter().zip(&disp.distances).map(|(n, d)| *n * *d).collect();
        Some(Self { corners, disp, n, vectors, blend: MultiBlend::of(disp) })
    }

    fn cells(&self) -> f64 {
        (self.n - 1) as f64
    }

    fn param(&self, p: DVec3) -> (f64, f64) {
        let [c0, c1, c2, c3] = self.corners;
        let (mut s, mut t) = (0.5, 0.5);
        for _ in 0..40 {
            let e0 = c0.lerp(c1, s);
            let e1 = c3.lerp(c2, s);
            let r = e0.lerp(e1, t) - p;
            let ds = (c1 - c0) * (1.0 - t) + (c2 - c3) * t;
            let dt = e1 - e0;
            let (a11, a12, a22) = (ds.dot(ds), ds.dot(dt), dt.dot(dt));
            let (b1, b2) = (ds.dot(r), dt.dot(r));
            let det = a11 * a22 - a12 * a12;
            if det.abs() < 1e-18 {
                break;
            }
            let x = (a22 * b1 - a12 * b2) / det;
            let y = (a11 * b2 - a12 * b1) / det;
            s -= x;
            t -= y;
            if x.abs() + y.abs() < 1e-13 {
                break;
            }
        }
        (s, t)
    }

    fn locate(&self, s: f64, t: f64) -> ([usize; 3], [f64; 3], usize) {
        let cells = self.n - 1;
        let x = s.clamp(0.0, 1.0) * cells as f64;
        let y = t.clamp(0.0, 1.0) * cells as f64;
        let ci = (x.floor() as usize).min(cells - 1);
        let cj = (y.floor() as usize).min(cells - 1);
        let (a, b) = (x - ci as f64, y - cj as f64);
        let w = self.n;
        let v = ci * w + cj;
        let quad = 2 * (ci * cells + cj);
        if v % 2 == 1 {
            if a + b <= 1.0 {
                ([v, v + 1, v + w], [1.0 - a - b, b, a], quad)
            } else {
                ([v + w + 1, v + 1, v + w], [a + b - 1.0, 1.0 - a, 1.0 - b], quad + 1)
            }
        } else if a >= b {
            ([v, v + w + 1, v + w], [1.0 - a, b, a - b], quad)
        } else {
            ([v, v + 1, v + w + 1], [1.0 - b, b - a, a], quad + 1)
        }
    }

    fn on_boundary(&self, a: DVec3, b: DVec3) -> bool {
        (0..4).any(|k| {
            let (p, q) = (self.corners[k], self.corners[(k + 1) % 4]);
            near_line(a, p, q) && near_line(b, p, q)
        })
    }

    fn preferred_power(&self, face: &[DVec3]) -> u32 {
        let cells = self.cells();
        let params: Vec<(f64, f64)> =
            face.iter().map(|v| self.param(*v)).map(|(s, t)| (s * cells, t * cells)).collect();
        let near_int = |x: f64| (x - x.round()).abs() < GRID_TOL;
        let mut aligned = params.iter().all(|(s, t)| near_int(*s) && near_int(*t));
        let (mut span_s, mut span_t, mut span) = (0.0f64, 0.0f64, 0.0f64);
        for k in 0..params.len() {
            let (a, b) = (params[k], params[(k + 1) % params.len()]);
            let (ds, dt) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
            if ds > GRID_TOL && dt > GRID_TOL {
                aligned = false;
            }
            span_s = span_s.max(ds);
            span_t = span_t.max(dt);
            span = span.max(ds.max(dt));
        }
        let pow2 = |x: f64| {
            let r = x.round() as u64;
            r > 0 && r.is_power_of_two()
        };
        if aligned && pow2(span_s) && pow2(span_t) {
            let need = span_s.max(span_t).round().max(1.0).log2().ceil() as u32;
            return need.clamp(MIN_CUT_DISP_POWER, MAX_DISP_POWER);
        }
        if aligned {
            return MAX_DISP_POWER;
        }
        let need = span.max(1.0).log2().ceil() as u32 + 1;
        need.clamp(MIN_CUT_DISP_POWER, MAX_DISP_POWER)
    }

    fn resample(&self, face: &[DVec3], power: u32) -> Option<DispInfo> {
        if face.len() != 4 || !(MIN_CUT_DISP_POWER..=MAX_DISP_POWER).contains(&power) {
            return None;
        }
        let start = face
            .iter()
            .copied()
            .min_by(|a, b| {
                let (pa, pb) = (self.param(*a), self.param(*b));
                (pa.0 + pa.1).total_cmp(&(pb.0 + pb.1))
            })
            .unwrap_or(face[0]);
        let (corners, normal) = source_corners(face, start).ok()?;
        let m = (1usize << power) + 1;
        let base = flat_grid(&corners, m);
        let mut out = self.disp.clone();
        out.power = power;
        out.start_position = start;
        let count = m * m;
        out.normals = Vec::with_capacity(count);
        out.distances = Vec::with_capacity(count);
        out.offsets = Vec::with_capacity(count);
        out.offset_normals = Vec::with_capacity(count);
        out.alphas = Vec::with_capacity(count);
        let mut blend = self.blend.as_ref().map(|_| MultiBlend::new(count));
        let fallback =
            self.disp.offset_normals.first().copied().filter(|n| n.length_squared() > 1e-12).unwrap_or(normal);
        for (k, p) in base.iter().enumerate() {
            let (s, t) = self.param(*p);
            let (idx, w, _) = self.locate(s, t);
            let mix3 = |src: &[DVec3]| -> DVec3 {
                idx.iter().zip(w).map(|(i, w)| src.get(*i).copied().unwrap_or(DVec3::ZERO) * w).sum()
            };
            let vector = mix3(&self.vectors);
            let length = vector.length();
            let direction = if length > 1e-9 {
                vector / length
            } else {
                mix3(&self.disp.normals).try_normalize().unwrap_or(normal)
            };
            out.normals.push(direction);
            out.distances.push(if length > 1e-9 { length } else { 0.0 });
            out.offsets.push(mix3(&self.disp.offsets));
            let on = if self.disp.offset_normals.len() == self.n * self.n {
                mix3(&self.disp.offset_normals).try_normalize().unwrap_or(fallback)
            } else {
                fallback
            };
            out.offset_normals.push(on);
            let alpha: f64 = idx.iter().zip(w).map(|(i, w)| self.disp.alphas.get(*i).copied().unwrap_or(0.0) * w).sum();
            out.alphas.push(alpha.clamp(0.0, 255.0));
            if let (Some(dst), Some(src)) = (blend.as_mut(), self.blend.as_ref()) {
                dst.weights[k] = mix_n(&src.weights, idx, w);
                dst.alphas[k] = mix_n(&src.alphas, idx, w);
                for (d, s) in dst.colors.iter_mut().zip(&src.colors) {
                    d[k] = mix_n(s, idx, w);
                }
            }
        }
        let indices = grid_indices(m);
        let mut tags = Vec::with_capacity(indices.len() / 3);
        for tri in indices.chunks_exact(3) {
            let centroid =
                tri.iter().map(|i| base.get(*i as usize).copied().unwrap_or(DVec3::ZERO)).sum::<DVec3>() / 3.0;
            let (s, t) = self.param(centroid);
            let (_, _, source_tri) = self.locate(s, t);
            let tag = self.disp.triangle_tags.get(source_tri).copied().unwrap_or(0);
            tags.push(tag & !(DISP_TAG_WALKABLE | DISP_TAG_BUILDABLE));
        }
        out.triangle_tags = tags;
        remove_multiblend(&mut out);
        if let Some(mut b) = blend {
            b.normalize();
            b.write_to(&mut out);
        }
        update_disp_tags(&mut out, face);
        Some(out)
    }
}

fn mix_n<const K: usize>(src: &[[f64; K]], idx: [usize; 3], w: [f64; 3]) -> [f64; K] {
    let mut out = [0.0; K];
    for (i, w) in idx.iter().zip(w) {
        let v = src.get(*i).copied().unwrap_or([0.0; K]);
        for (o, x) in out.iter_mut().zip(v) {
            *o += x * w;
        }
    }
    out
}

fn near_line(p: DVec3, a: DVec3, b: DVec3) -> bool {
    let d = b - a;
    let len2 = d.length_squared();
    if len2 < 1e-12 {
        return p.distance(a) < EDGE_TOL;
    }
    let t = (p - a).dot(d) / len2;
    (-1e-6..=1.0 + 1e-6).contains(&t) && p.distance(a + d * t) < EDGE_TOL
}

fn area(poly: &[DVec3]) -> f64 {
    polygon_area_vector(poly).length()
}

fn chord_plane(a: DVec3, b: DVec3, normal: DVec3) -> Option<Plane> {
    let n = (b - a).cross(normal).try_normalize()?;
    Plane::new(n, n.dot(a))
}

fn chamfer_plane(src: &Source<'_>, poly: &[DVec3], normal: DVec3) -> Option<Plane> {
    let m = poly.len();
    let at = |i: usize| poly[i % m];
    let angle = |i: usize| {
        let (p, v, q) = (at(i + m - 1), at(i), at(i + 1));
        (p - v).angle_between(q - v)
    };
    let corner = (0..m).find(|&i| src.on_boundary(at(i + m - 1), at(i)) && src.on_boundary(at(i), at(i + 1)));
    let i = corner.or_else(|| (0..m).max_by(|a, b| angle(*a).total_cmp(&angle(*b))))?;
    let (p, v, q) = (at(i + m - 1), at(i), at(i + 1));
    let eps = CHAMFER.min(0.25 * p.distance(v).min(q.distance(v)));
    if eps < MIN_CHAMFER {
        return None;
    }
    let a = v + (p - v).normalize() * eps;
    let b = v + (q - v).normalize() * eps;
    let plane = chord_plane(a, b, normal)?;
    if plane.distance(v) > 0.0 { Some(plane) } else { Some(plane.flip()) }
}

fn snap_to_grid(src: &Source<'_>, a: DVec3, b: DVec3) -> DVec3 {
    let mid = (a + b) * 0.5;
    let cells = src.cells();
    let (s, t) = src.param(mid);
    let (s, t) = ((s * cells).round() / cells, (t * cells).round() / cells);
    let [c0, c1, c2, c3] = src.corners;
    let snapped = c0.lerp(c1, s).lerp(c3.lerp(c2, s), t);
    let len = a.distance(b);
    let along = (snapped - a).dot((b - a) / len.max(1e-12));
    if near_line(snapped, a, b) && along > 0.2 * len && along < 0.8 * len { snapped } else { mid }
}

fn split_plane(src: &Source<'_>, poly: &[DVec3], normal: DVec3) -> Option<Plane> {
    let m = poly.len();
    let at = |i: usize| poly[i % m];
    let mut best: Option<(f64, Plane)> = None;
    let mut consider = |quad: [DVec3; 4], from: DVec3, to: DVec3, rest: f64| {
        let qa = area(&quad);
        let score = qa.min(rest - qa);
        if score > 1e-6
            && best.as_ref().is_none_or(|(s, _)| score > *s)
            && let Some(plane) = chord_plane(from, to, normal)
        {
            best = Some((score, plane));
        }
    };
    let total = area(poly);
    if m.is_multiple_of(2) {
        for r in 0..m {
            let quad = [at(r), at(r + 1), at(r + 2), at(r + 3)];
            consider(quad, at(r), at(r + 3), total);
        }
    } else {
        let boundary: Vec<usize> = (0..m).filter(|&e| src.on_boundary(at(e), at(e + 1))).collect();
        let edges: Vec<usize> = if boundary.is_empty() { (0..m).collect() } else { boundary };
        for e in edges {
            let p = snap_to_grid(src, at(e), at(e + 1));
            let far = at(e + m - 2);
            consider([far, at(e + m - 1), at(e), p], p, far, total);
        }
    }
    best.map(|(_, p)| p)
}

enum Fix {
    Keep,
    Split(Plane),
    Chamfer(Plane),
    GiveUp,
}

fn next_fix(sources: &[Option<Source<'_>>], piece: &ClipPiece, geom: &SolidGeom) -> Fix {
    for f in &geom.faces {
        let Some(Some(j)) = piece.origins.get(f.side_index) else { continue };
        let Some(Some(src)) = sources.get(*j) else { continue };
        let m = f.vertices.len();
        if m == 4 {
            continue;
        }
        let normal = f.plane.normal;
        let plane = if m == 3 {
            chamfer_plane(src, &f.vertices, normal).map(Fix::Chamfer)
        } else {
            split_plane(src, &f.vertices, normal).map(Fix::Split)
        };
        return plane.unwrap_or(Fix::GiveUp);
    }
    Fix::Keep
}

pub fn keep_cut_displacements(
    original: &[Side],
    pieces: Vec<ClipPiece>,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Vec<ClipPiece> {
    let og = build_solid(original);
    if !og.valid {
        return pieces;
    }
    let sources: Vec<Option<Source<'_>>> = original
        .iter()
        .enumerate()
        .map(|(j, side)| {
            let disp = side.disp.as_ref()?;
            let face = og.face_for_side(j)?;
            Source::new(&face.vertices, disp)
        })
        .collect();
    if sources.iter().all(Option::is_none) {
        return pieces;
    }
    let mut queue = pieces;
    let mut done: Vec<ClipPiece> = Vec::with_capacity(queue.len());
    let mut steps = 0usize;
    while let Some(piece) = queue.pop() {
        steps += 1;
        let geom = build_solid(&piece.sides);
        if steps > MAX_STEPS || !geom.valid {
            done.push(piece);
            continue;
        }
        match next_fix(&sources, &piece, &geom) {
            Fix::Keep | Fix::GiveUp => done.push(piece),
            Fix::Split(plane) => match clip_geom(&piece.sides, &geom, &plane, None, next_side_id) {
                Ok((Some(front), Some(back))) => {
                    queue.push(front.compose(&piece.origins));
                    queue.push(back.compose(&piece.origins));
                }
                _ => done.push(piece),
            },
            Fix::Chamfer(plane) => match clip_geom(&piece.sides, &geom, &plane, None, next_side_id) {
                Ok((_, Some(back))) => queue.push(back.compose(&piece.origins)),
                _ => done.push(piece),
            },
        }
    }
    done.reverse();
    let geoms: Vec<SolidGeom> = done.iter().map(|p| build_solid(&p.sides)).collect();
    let mut powers = vec![0u32; original.len()];
    for (piece, geom) in done.iter().zip(&geoms) {
        for f in &geom.faces {
            let Some(Some(j)) = piece.origins.get(f.side_index) else { continue };
            let Some(Some(src)) = sources.get(*j) else { continue };
            let side = &piece.sides[f.side_index];
            if side.disp.is_none() && f.vertices.len() == 4 {
                powers[*j] = powers[*j].max(src.preferred_power(&f.vertices));
            }
        }
    }
    for (piece, geom) in done.iter_mut().zip(&geoms) {
        for f in &geom.faces {
            let Some(Some(j)) = piece.origins.get(f.side_index).copied() else { continue };
            let Some(Some(src)) = sources.get(j) else { continue };
            let Some(side) = piece.sides.get_mut(f.side_index) else { continue };
            if side.disp.is_none() && f.vertices.len() == 4 && powers[j] > 0 {
                side.disp = src.resample(&f.vertices, powers[j]).map(Box::new);
            }
        }
    }
    done
}

pub fn displaced_surface_point(face: &[DVec3], disp: &DispInfo, p: DVec3) -> Option<DVec3> {
    let src = Source::new(face, disp)?;
    let (s, t) = src.param(p);
    if !(-1e-6..=1.0 + 1e-6).contains(&s) || !(-1e-6..=1.0 + 1e-6).contains(&t) {
        return None;
    }
    let (idx, w, _) = src.locate(s, t);
    let [c0, c1, c2, c3] = src.corners;
    let normal = crate::util::polygon_normal(face)?;
    let elevation = if disp.elevation.is_finite() { normal * disp.elevation } else { DVec3::ZERO };
    let n = src.n;
    let mut out = DVec3::ZERO;
    for (i, w) in idx.iter().zip(w) {
        let (r, c) = (i / n, i % n);
        let (fs, ft) = (r as f64 / (n - 1) as f64, c as f64 / (n - 1) as f64);
        let base = c0.lerp(c1, fs).lerp(c3.lerp(c2, fs), ft);
        let v = base + src.vectors[*i] + disp.offsets[*i] + elevation;
        out += v * w;
    }
    Some(out)
}

#[cfg(test)]
mod tests;
