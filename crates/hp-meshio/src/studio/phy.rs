use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use glam::DVec3;

use super::buf::Buf;
use super::mdl::Info;
use crate::Model;
use crate::smd::Collision as Kind;

const METERS_PER_UNIT: f64 = 0.0254;
pub(crate) const MARGIN: f64 = 0.25;
const MAX_SHRINK_PLANES: usize = 256;
const MAX_PIECES: usize = 40;
const MAX_PIECE_TRIANGLES: usize = 1000;
const MAX_HULL_INPUT: usize = 20_000;
const HULL_EPS: [f64; 3] = [0.01, 0.1, 0.5];
const FLAT_THICKNESS: f64 = 1.0;
const MAX_TRIANGLES: usize = 4095;
const MAX_LEDGE_POINTS: usize = 65535;
const VPHY_HEADER: usize = 28;
const SURFACE_HEADER: usize = 48;
const LEDGE_HEADER: usize = 16;
const TRIANGLE: usize = 16;
const POINT: usize = 16;
const NODE: usize = 28;
const IVPS: u32 = 0x5350_5649;

pub(crate) struct Collision {
    pub blob: Vec<u8>,
    pub lo: DVec3,
    pub hi: DVec3,
    pub volume: f64,
    pub pieces: usize,
}

fn groups(model: &Model, kind: Kind, given: Option<&[Vec<DVec3>]>) -> Option<Vec<Vec<DVec3>>> {
    if let Some(given) = given {
        return Some(given.to_vec());
    }
    Some(match kind {
        Kind::None => return None,
        Kind::Box => {
            let (lo, hi) = model.bounds()?;
            let (lo, hi) = thicken_box(lo, hi);
            vec![(0..8).map(|i| corner(lo, hi, i)).collect()]
        }
        Kind::Pieces => pieces(model),
        Kind::Hull | Kind::Decompose => {
            crate::collision::generated(model, kind, crate::collision::CollisionLimits::default())?
        }
    })
}

pub(crate) fn preview(model: &Model, kind: Kind, given: Option<&[Vec<DVec3>]>) -> Vec<Vec<DVec3>> {
    groups(model, kind, given)
        .unwrap_or_default()
        .iter()
        .filter_map(|g| piece_hull(g))
        .map(|h| h.verts)
        .filter(|v| v.len() >= 4)
        .collect()
}

pub(crate) fn build(model: &Model, kind: Kind, given: Option<&[Vec<DVec3>]>) -> Option<Collision> {
    let groups = groups(model, kind, given)?;
    let mut hulls: Vec<Hull> = groups.iter().filter_map(|g| piece_hull(g)).map(|h| shrink(&h)).collect();
    if hulls.is_empty() {
        let (lo, hi) = model.bounds()?;
        let (lo, hi) = thicken_box(lo, hi);
        let pts: Vec<DVec3> = (0..8).map(|i| corner(lo, hi, i)).collect();
        hulls.push(convex_hull(&pts)?);
    }
    let mut ledges = Vec::new();
    let mut kept = Vec::new();
    for h in hulls {
        if let Some(l) = make_ledge(&h) {
            ledges.push(l);
            kept.push(h);
        }
    }
    if ledges.is_empty() {
        return None;
    }
    let root = if ledges.len() > 1 { root_hull(&kept) } else { None };
    let blob = surface_blob(&ledges, root.as_ref(), &drag_areas(&kept))?;
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    let mut volume = 0.0;
    for h in &kept {
        for v in &h.verts {
            lo = lo.min(*v);
            hi = hi.max(*v);
        }
        volume += hull_volume(h);
    }
    Some(Collision { blob, lo, hi, volume, pieces: kept.len() })
}

pub(crate) fn file(c: &Collision, info: &Info, name: &str) -> Vec<u8> {
    let mut b = Buf::default();
    b.i32(16);
    b.i32(0);
    b.i32(1);
    b.i32(info.checksum);
    b.i32(c.blob.len() as i32);
    b.b.extend_from_slice(&c.blob);
    let mass = info.mass as f32;
    let mut text = String::new();
    let _ = write!(
        text,
        "solid {{\n\"index\" \"0\"\n\"name\" \"{name}\"\n\"mass\" \"{mass:.6}\"\n\"surfaceprop\" \"{}\"\n\"damping\" \"0.000000\"\n\"rotdamping\" \"0.000000\"\n\"inertia\" \"1.000000\"\n\"volume\" \"{:.6}\"\n}}\n",
        info.surface_prop, c.volume as f32
    );
    let _ = write!(text, "editparams {{\n\"rootname\" \"\"\n\"totalmass\" \"{mass:.6}\"\n");
    if c.pieces > 1 {
        text.push_str("\"concave\" \"1\"\n");
    }
    text.push_str("}\n");
    b.b.extend_from_slice(text.as_bytes());
    b.u8(0);
    b.b
}

fn corner(lo: DVec3, hi: DVec3, i: usize) -> DVec3 {
    DVec3::new(
        if i & 1 == 0 { lo.x } else { hi.x },
        if i & 2 == 0 { lo.y } else { hi.y },
        if i & 4 == 0 { lo.z } else { hi.z },
    )
}

fn thicken_box(lo: DVec3, hi: DVec3) -> (DVec3, DVec3) {
    let grow = (DVec3::splat(FLAT_THICKNESS) - (hi - lo)).max(DVec3::ZERO) * 0.5;
    (lo - grow, hi + grow)
}

fn pieces(model: &Model) -> Vec<Vec<DVec3>> {
    let mut ids: HashMap<[i64; 3], usize> = HashMap::new();
    let mut points: Vec<DVec3> = Vec::new();
    let mut tris: Vec<[usize; 3]> = Vec::new();
    for t in &model.triangles {
        if !t.pos.iter().all(|p| p.is_finite()) {
            continue;
        }
        let c = t.pos.map(|p| {
            let k = [(p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64, (p.z * 100.0).round() as i64];
            *ids.entry(k).or_insert_with(|| {
                points.push(p);
                points.len() - 1
            })
        });
        tris.push(c);
    }
    let mut parent: Vec<usize> = (0..points.len()).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for t in &tris {
        let a = find(&mut parent, t[0]);
        for &v in &t[1..] {
            let b = find(&mut parent, v);
            if a != b {
                parent[b] = a;
            }
        }
    }
    let mut piece_of: HashMap<usize, usize> = HashMap::new();
    let mut piece_points: Vec<Vec<usize>> = Vec::new();
    let mut piece_normal: Vec<DVec3> = Vec::new();
    for i in 0..points.len() {
        let root = find(&mut parent, i);
        let n = piece_of.len();
        let p = *piece_of.entry(root).or_insert(n);
        if p == piece_points.len() {
            piece_points.push(Vec::new());
            piece_normal.push(DVec3::ZERO);
        }
        piece_points[p].push(i);
    }
    for t in &tris {
        let p = piece_of[&find(&mut parent, t[0])];
        let n = (points[t[1]] - points[t[0]]).cross(points[t[2]] - points[t[0]]);
        let acc = piece_normal[p];
        piece_normal[p] += if acc.dot(n) < 0.0 { -n } else { n };
    }
    let mut out: Vec<(Vec<DVec3>, DVec3)> = piece_points
        .iter()
        .zip(&piece_normal)
        .map(|(ids, n)| (ids.iter().map(|&i| points[i]).collect(), n.normalize_or_zero()))
        .collect();
    if out.len() > MAX_PIECES {
        out = group_pieces(out);
    }
    out.into_iter().map(|(pts, n)| with_thickness(pts, n)).collect()
}

fn with_thickness(pts: Vec<DVec3>, normal: DVec3) -> Vec<DVec3> {
    if normal == DVec3::ZERO || pts.len() < 3 {
        return pts;
    }
    let ds: Vec<f64> = pts.iter().map(|p| p.dot(normal)).collect();
    let (dmin, dmax) = ds.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &d| (a.min(d), b.max(d)));
    if dmax - dmin >= FLAT_THICKNESS {
        return pts;
    }
    let half = (FLAT_THICKNESS - (dmax - dmin)) * 0.5;
    let mut out = Vec::with_capacity(pts.len() * 2);
    for p in pts {
        out.push(p + normal * half);
        out.push(p - normal * half);
    }
    out
}

fn group_pieces(pieces: Vec<(Vec<DVec3>, DVec3)>) -> Vec<(Vec<DVec3>, DVec3)> {
    let centers: Vec<DVec3> = pieces
        .iter()
        .map(|(p, _)| {
            let (lo, hi) =
                p.iter().fold((DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)), |(a, b), v| {
                    (a.min(*v), b.max(*v))
                });
            (lo + hi) * 0.5
        })
        .collect();
    let weights: Vec<f64> = pieces.iter().map(|(p, _)| p.len() as f64).collect();
    let k = MAX_PIECES;
    let first = (0..pieces.len()).max_by(|&a, &b| weights[a].total_cmp(&weights[b])).unwrap_or(0);
    let mut seeds = vec![centers[first]];
    let mut dist: Vec<f64> = centers.iter().map(|c| c.distance_squared(seeds[0])).collect();
    while seeds.len() < k {
        let next = (0..centers.len()).max_by(|&a, &b| dist[a].total_cmp(&dist[b])).unwrap_or(0);
        if dist[next] <= 0.0 {
            break;
        }
        seeds.push(centers[next]);
        for (d, c) in dist.iter_mut().zip(&centers) {
            *d = d.min(c.distance_squared(centers[next]));
        }
    }
    let mut assign = vec![0usize; centers.len()];
    for _ in 0..12 {
        for (i, c) in centers.iter().enumerate() {
            assign[i] = (0..seeds.len())
                .min_by(|&a, &b| c.distance_squared(seeds[a]).total_cmp(&c.distance_squared(seeds[b])))
                .unwrap_or(0);
        }
        let mut sum = vec![DVec3::ZERO; seeds.len()];
        let mut w = vec![0.0; seeds.len()];
        for (i, &a) in assign.iter().enumerate() {
            sum[a] += centers[i] * weights[i];
            w[a] += weights[i];
        }
        for (s, (sum, w)) in seeds.iter_mut().zip(sum.iter().zip(&w)) {
            if *w > 0.0 {
                *s = *sum / *w;
            }
        }
    }
    let mut groups: Vec<(Vec<DVec3>, DVec3)> = vec![(Vec::new(), DVec3::ZERO); seeds.len()];
    for ((pts, n), &a) in pieces.into_iter().zip(&assign) {
        groups[a].0.extend(pts);
        groups[a].1 = if groups[a].1 == DVec3::ZERO { n } else { groups[a].1 };
    }
    groups.retain(|g| !g.0.is_empty());
    groups
}

fn piece_hull(points: &[DVec3]) -> Option<Hull> {
    let mut pts = dedup(points);
    if pts.len() > MAX_HULL_INPUT {
        pts = thin(&pts, 128);
    }
    let mut hull = convex_hull(&pts)?;
    let mut cells = 64;
    while hull.tris.len() > MAX_PIECE_TRIANGLES && cells >= 4 {
        let thinner = thin(&hull.verts, cells);
        match convex_hull(&thinner) {
            Some(h) => hull = h,
            None => break,
        }
        cells /= 2;
    }
    (hull.tris.len() <= MAX_TRIANGLES).then_some(hull)
}

fn shrink(h: &Hull) -> Hull {
    let (lo, hi) = h
        .verts
        .iter()
        .fold((DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)), |(a, b), v| (a.min(*v), b.max(*v)));
    if (hi - lo).min_element() < 4.0 * MARGIN + 0.01 {
        return h.clone();
    }
    let planes = hull_planes(h);
    let shrunk = if planes.len() <= MAX_SHRINK_PLANES {
        shrink_planes(h, &planes)
    } else {
        let mut normals = vec![DVec3::ZERO; h.verts.len()];
        for t in &h.tris {
            let [a, b, c] = t.map(|i| h.verts[i as usize]);
            let n = (b - a).cross(c - a).normalize_or_zero();
            for &i in t {
                normals[i as usize] += n;
            }
        }
        let pts: Vec<DVec3> = h.verts.iter().zip(&normals).map(|(v, n)| *v - n.normalize_or_zero() * MARGIN).collect();
        convex_hull(&pts)
    };
    shrunk.filter(|s| s.tris.len() <= MAX_TRIANGLES).unwrap_or_else(|| h.clone())
}

fn hull_planes(h: &Hull) -> Vec<(DVec3, f64)> {
    let mut planes: Vec<(DVec3, f64)> = Vec::new();
    for t in &h.tris {
        let [a, b, c] = t.map(|i| h.verts[i as usize]);
        let n = (b - a).cross(c - a).normalize_or_zero();
        if n == DVec3::ZERO {
            continue;
        }
        let d = n.dot(a);
        if !planes.iter().any(|(m, e)| m.dot(n) > 1.0 - 1e-6 && (e - d).abs() < HULL_EPS[0]) {
            planes.push((n, d));
        }
    }
    planes
}

fn shrink_planes(h: &Hull, planes: &[(DVec3, f64)]) -> Option<Hull> {
    if planes.len() < 4 {
        return None;
    }
    let center = h.verts.iter().copied().sum::<DVec3>() / h.verts.len() as f64;
    let radius = h.verts.iter().map(|v| v.distance(center)).fold(0.0, f64::max);
    let size = radius * 4.0 + 64.0;
    let mut pts = Vec::new();
    for (i, &(n, d)) in planes.iter().enumerate() {
        let d = d - MARGIN;
        let origin = center - n * (n.dot(center) - d);
        let u = if n.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        let u = (u - n * u.dot(n)).normalize();
        let v = n.cross(u);
        let mut poly =
            vec![origin + (u + v) * size, origin + (u - v) * size, origin - (u + v) * size, origin + (v - u) * size];
        for (j, &(m, e)) in planes.iter().enumerate() {
            if j != i {
                poly = clip_polygon(&poly, m, e - MARGIN);
                if poly.len() < 3 {
                    break;
                }
            }
        }
        if poly.len() >= 3 {
            pts.extend(poly);
        }
    }
    convex_hull(&pts)
}

fn clip_polygon(poly: &[DVec3], n: DVec3, d: f64) -> Vec<DVec3> {
    let mut out = Vec::with_capacity(poly.len() + 1);
    for (i, &a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        let (da, db) = (n.dot(a) - d, n.dot(b) - d);
        if da <= 0.0 {
            out.push(a);
        }
        if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
            out.push(a + (b - a) * (da / (da - db)));
        }
    }
    out
}

fn dedup(points: &[DVec3]) -> Vec<DVec3> {
    let mut seen = HashSet::new();
    points
        .iter()
        .copied()
        .filter(|p| p.is_finite() && seen.insert(((p.x * 100.0) as i64, (p.y * 100.0) as i64, (p.z * 100.0) as i64)))
        .collect()
}

fn thin(points: &[DVec3], cells: usize) -> Vec<DVec3> {
    let (lo, hi) = points
        .iter()
        .fold((DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)), |(a, b), v| (a.min(*v), b.max(*v)));
    let center = (lo + hi) * 0.5;
    let size = (hi - lo).max_element().max(1e-6) / cells as f64;
    let mut best: HashMap<(i64, i64, i64), DVec3> = HashMap::new();
    for &p in points {
        let c = ((p - lo) / size).floor();
        let k = (c.x as i64, c.y as i64, c.z as i64);
        let e = best.entry(k).or_insert(p);
        if p.distance_squared(center) > e.distance_squared(center) {
            *e = p;
        }
    }
    let mut out: Vec<DVec3> = best.into_values().collect();
    out.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)).then(a.z.total_cmp(&b.z)));
    out
}

fn drag_areas(hulls: &[Hull]) -> [f32; 3] {
    let mut a = DVec3::ZERO;
    for h in hulls {
        for t in &h.tris {
            let [p, q, r] = t.map(|i| h.verts[i as usize]);
            a += ((q - p).cross(r - p) * 0.5).abs() * 0.5;
        }
    }
    let m2 = METERS_PER_UNIT * METERS_PER_UNIT;
    [(a.x * m2) as f32, (a.z * m2) as f32, (a.y * m2) as f32]
}

#[derive(Clone, Debug)]
struct Hull {
    verts: Vec<DVec3>,
    tris: Vec<[u32; 3]>,
}

fn convex_hull(points: &[DVec3]) -> Option<Hull> {
    let pts = dedup(points);
    if pts.len() < 4 {
        return None;
    }
    HULL_EPS.iter().find_map(|&eps| hull_with(&pts, eps))
}

#[derive(Clone, Copy)]
struct HullFace {
    v: [u32; 3],
    n: DVec3,
    d: f64,
    alive: bool,
}

fn hull_with(p: &[DVec3], eps: f64) -> Option<Hull> {
    let n = p.len();
    let argmax = |f: &dyn Fn(usize) -> f64| (0..n).max_by(|&a, &b| f(a).total_cmp(&f(b)));
    let i0 = (0..n).min_by(|&a, &b| p[a].x.total_cmp(&p[b].x))?;
    let i1 = argmax(&|i| p[i].distance_squared(p[i0]))?;
    if p[i1].distance(p[i0]) <= eps {
        return None;
    }
    let dir = (p[i1] - p[i0]).normalize();
    let line = |i: usize| {
        let v = p[i] - p[i0];
        (v - dir * v.dot(dir)).length()
    };
    let i2 = argmax(&line)?;
    if line(i2) <= eps {
        return None;
    }
    let normal = (p[i1] - p[i0]).cross(p[i2] - p[i0]).normalize();
    let plane = |i: usize| (p[i] - p[i0]).dot(normal).abs();
    let i3 = argmax(&plane)?;
    if plane(i3) <= eps {
        return None;
    }
    let inside = (p[i0] + p[i1] + p[i2] + p[i3]) * 0.25;
    let mut faces: Vec<HullFace> = Vec::new();
    let mut edges: HashMap<(u32, u32), usize> = HashMap::new();
    let ids = [i0, i1, i2, i3].map(|i| i as u32);
    for [a, b, c] in [[0, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
        let (a, mut b, mut c) = (ids[a], ids[b], ids[c]);
        let nrm = (p[b as usize] - p[a as usize]).cross(p[c as usize] - p[a as usize]);
        if nrm.dot(inside - p[a as usize]) > 0.0 {
            std::mem::swap(&mut b, &mut c);
        }
        add_face(&mut faces, &mut edges, p, [a, b, c], inside)?;
    }
    let mut stamp: Vec<usize> = Vec::new();
    let mut visible = Vec::new();
    let mut horizon = Vec::new();
    for (qi, &q) in p.iter().enumerate() {
        if ids.contains(&(qi as u32)) {
            continue;
        }
        let dist = |f: &HullFace| f.n.dot(q) - f.d;
        let Some(first) = (0..faces.len())
            .filter(|&f| faces[f].alive)
            .max_by(|&a, &b| dist(&faces[a]).total_cmp(&dist(&faces[b])))
            .filter(|&f| dist(&faces[f]) > eps)
        else {
            continue;
        };
        stamp.resize(faces.len(), usize::MAX);
        visible.clear();
        visible.push(first);
        stamp[first] = qi;
        let mut k = 0;
        while k < visible.len() {
            let [a, b, c] = faces[visible[k]].v;
            for (x, y) in [(a, b), (b, c), (c, a)] {
                let g = *edges.get(&(y, x))?;
                if stamp[g] != qi && dist(&faces[g]) > eps * 1e-6 {
                    stamp[g] = qi;
                    visible.push(g);
                }
            }
            k += 1;
        }
        horizon.clear();
        for &f in &visible {
            let [a, b, c] = faces[f].v;
            for (x, y) in [(a, b), (b, c), (c, a)] {
                let g = *edges.get(&(y, x))?;
                if stamp.get(g) != Some(&qi) {
                    horizon.push((x, y));
                }
            }
        }
        for &f in &visible {
            faces[f].alive = false;
            let [a, b, c] = faces[f].v;
            for e in [(a, b), (b, c), (c, a)] {
                edges.remove(&e);
            }
        }
        if horizon.len() < 3 {
            return None;
        }
        let mut starts: Vec<u32> = horizon.iter().map(|e| e.0).collect();
        starts.sort_unstable();
        if starts.windows(2).any(|w| w[0] == w[1]) {
            return None;
        }
        for &(x, y) in &horizon {
            add_face(&mut faces, &mut edges, p, [x, y, qi as u32], inside)?;
        }
        if faces.len() > 4096 && faces.len() > 4 * faces.iter().filter(|f| f.alive).count() {
            compact(&mut faces, &mut edges);
            stamp.clear();
        }
    }
    let alive: Vec<&HullFace> = faces.iter().filter(|f| f.alive).collect();
    let mut used: Vec<u32> = Vec::new();
    for f in &alive {
        let [a, b, c] = f.v;
        for (x, y) in [(a, b), (b, c), (c, a)] {
            let g = *edges.get(&(y, x))?;
            if !faces.get(g)?.alive {
                return None;
            }
        }
        used.extend_from_slice(&f.v);
    }
    used.sort_unstable();
    used.dedup();
    for f in &alive {
        if used.iter().any(|&v| f.n.dot(p[v as usize]) - f.d > 4.0 * eps) {
            return None;
        }
    }
    let remap: HashMap<u32, u32> = used.iter().enumerate().map(|(i, &v)| (v, i as u32)).collect();
    let verts = used.iter().map(|&v| p[v as usize]).collect();
    let tris = alive.iter().map(|f| f.v.map(|v| remap[&v])).collect();
    Some(Hull { verts, tris })
}

fn compact(faces: &mut Vec<HullFace>, edges: &mut HashMap<(u32, u32), usize>) {
    faces.retain(|f| f.alive);
    edges.clear();
    for (i, f) in faces.iter().enumerate() {
        let [a, b, c] = f.v;
        for e in [(a, b), (b, c), (c, a)] {
            edges.insert(e, i);
        }
    }
}

fn add_face(
    faces: &mut Vec<HullFace>,
    edges: &mut HashMap<(u32, u32), usize>,
    p: &[DVec3],
    v: [u32; 3],
    inside: DVec3,
) -> Option<()> {
    let [a, b, c] = v.map(|i| p[i as usize]);
    let n = (b - a).cross(c - a);
    let len = n.length();
    if len.is_nan() || len <= 1e-12 {
        return None;
    }
    let n = n / len;
    let d = n.dot(a);
    if n.dot(inside) - d >= 0.0 {
        return None;
    }
    let f = faces.len();
    for e in [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])] {
        if edges.insert(e, f).is_some() {
            return None;
        }
    }
    faces.push(HullFace { v, n, d, alive: true });
    Some(())
}

fn hull_volume(h: &Hull) -> f64 {
    let r = h.verts.iter().copied().sum::<DVec3>() / h.verts.len().max(1) as f64;
    h.tris
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| h.verts[i as usize] - r);
            a.dot(b.cross(c)) / 6.0
        })
        .sum()
}

#[derive(Clone, Debug)]
struct Ledge {
    points: Vec<[f32; 3]>,
    tris: Vec<[u16; 3]>,
    twins: Vec<[i16; 3]>,
    pierce: Vec<u16>,
    is_virtual: bool,
}

impl Ledge {
    fn point(&self, i: usize) -> DVec3 {
        let p = self.points[i];
        DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]))
    }
}

fn to_ivp(p: DVec3) -> [f32; 3] {
    [(p.x * METERS_PER_UNIT) as f32, (-p.z * METERS_PER_UNIT) as f32, (p.y * METERS_PER_UNIT) as f32]
}

fn make_ledge(h: &Hull) -> Option<Ledge> {
    if h.tris.len() < 4 || h.tris.len() > MAX_TRIANGLES || h.verts.len() > MAX_LEDGE_POINTS {
        return None;
    }
    let tris: Vec<[u16; 3]> = h.tris.iter().map(|t| t.map(|v| v as u16)).collect();
    let mut edge_at: HashMap<(u16, u16), usize> = HashMap::with_capacity(tris.len() * 3);
    for (t, tri) in tris.iter().enumerate() {
        for e in 0..3 {
            edge_at.insert((tri[e], tri[(e + 1) % 3]), t * 4 + 1 + e);
        }
    }
    let mut twins = Vec::with_capacity(tris.len());
    for (t, tri) in tris.iter().enumerate() {
        let mut row = [0i16; 3];
        for (e, slot) in row.iter_mut().enumerate() {
            let other = *edge_at.get(&(tri[(e + 1) % 3], tri[e]))?;
            let off = other as i64 - (t * 4 + 1 + e) as i64;
            if !(-16384..16384).contains(&off) {
                return None;
            }
            *slot = off as i16;
        }
        twins.push(row);
    }
    let pierce = pierce_indices(h);
    Some(Ledge { points: h.verts.iter().map(|&v| to_ivp(v)).collect(), tris, twins, pierce, is_virtual: false })
}

fn root_hull(hulls: &[Hull]) -> Option<Ledge> {
    let points: Vec<DVec3> = hulls.iter().flat_map(|h| h.verts.iter().copied()).collect();
    let hull = piece_hull(&points)?;
    let mut ledge = make_ledge(&hull)?;
    ledge.is_virtual = true;
    Some(ledge)
}

fn pierce_indices(h: &Hull) -> Vec<u16> {
    let planes: Vec<(DVec3, f64, DVec3)> = h
        .tris
        .iter()
        .map(|t| {
            let [a, b, c] = t.map(|i| h.verts[i as usize]);
            let n = (b - a).cross(c - a).normalize_or_zero();
            (n, n.dot(a), (a + b + c) / 3.0)
        })
        .collect();
    let mut out = Vec::with_capacity(planes.len());
    for (t, &(n, _, center)) in planes.iter().enumerate() {
        let dir = -n;
        let mut best: Option<(f64, usize)> = None;
        for (j, &(m, d, _)) in planes.iter().enumerate() {
            let den = m.dot(dir);
            if j == t || den <= 1e-9 {
                continue;
            }
            let s = (d - m.dot(center)) / den;
            if best.is_none_or(|(bs, _)| s < bs) {
                best = Some((s, j));
            }
        }
        let pick = match best {
            Some((s, j0)) => {
                let hit = center + dir * s;
                let mut pick = j0;
                let mut score = f64::NEG_INFINITY;
                for (j, &(m, d, _)) in planes.iter().enumerate() {
                    if j == t || m.dot(dir) <= 1e-9 || (m.dot(hit) - d).abs() > 1e-6 * (1.0 + s.abs()) {
                        continue;
                    }
                    let [a, b, c] = h.tris[j].map(|i| h.verts[i as usize]);
                    let inside = [(a, b), (b, c), (c, a)]
                        .iter()
                        .map(|&(x, y)| (y - x).cross(hit - x).dot(m))
                        .fold(f64::INFINITY, f64::min);
                    if inside > score {
                        score = inside;
                        pick = j;
                    }
                }
                pick
            }
            None => t,
        };
        out.push(pick as u16);
    }
    out
}

fn ledge_moments(l: &Ledge) -> (f64, DVec3, DVec3) {
    let n = l.points.len().max(1) as f64;
    let r = (0..l.points.len()).map(|i| l.point(i)).sum::<DVec3>() / n;
    let (mut v, mut s1, mut s2) = (0.0, DVec3::ZERO, DVec3::ZERO);
    for t in &l.tris {
        let [a, b, c] = t.map(|i| l.point(usize::from(i)) - r);
        let tv = a.dot(b.cross(c)) / 6.0;
        v += tv;
        s1 += tv * (a + b + c) / 4.0;
        s2 += tv / 10.0 * (a * a + b * b + c * c + a * b + b * c + c * a);
    }
    (v, s1 + v * r, s2 + 2.0 * r * s1 + v * r * r)
}

#[derive(Clone, Copy, Debug, Default)]
struct TreeNode {
    right: usize,
    ledge: usize,
    center: [f32; 3],
    radius: f32,
    box_sizes: [u8; 3],
}

fn bound_sphere(ledges: &[Ledge], items: &[usize]) -> ([f32; 3], f32, [u8; 3]) {
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for &l in items {
        for i in 0..ledges[l].points.len() {
            let p = ledges[l].point(i);
            lo = lo.min(p);
            hi = hi.max(p);
        }
    }
    let c = (lo + hi) * 0.5;
    let center = [c.x as f32, c.y as f32, c.z as f32];
    let c = DVec3::new(f64::from(center[0]), f64::from(center[1]), f64::from(center[2]));
    let (mut r, mut ext) = (0.0f64, DVec3::ZERO);
    for &l in items {
        for i in 0..ledges[l].points.len() {
            let d = ledges[l].point(i) - c;
            r = r.max(d.length());
            ext = ext.max(d.abs());
        }
    }
    let radius = (r * (1.0 + 1e-6) + 1e-6) as f32;
    let rr = f64::from(radius);
    let size = |e: f64| if rr > 0.0 { ((e / rr * 250.0).floor() + 1.0).clamp(0.0, 255.0) as u8 } else { 0 };
    (center, radius, [size(ext.x), size(ext.y), size(ext.z)])
}

fn build_tree(ledges: &[Ledge], centers: &[DVec3], items: &mut [usize], out: &mut Vec<TreeNode>) {
    let (center, radius, box_sizes) = bound_sphere(ledges, items);
    let me = out.len();
    out.push(TreeNode { right: 0, ledge: items.first().copied().unwrap_or(0), center, radius, box_sizes });
    if items.len() < 2 {
        return;
    }
    let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
    for &i in items.iter() {
        lo = lo.min(centers[i]);
        hi = hi.max(centers[i]);
    }
    let size = hi - lo;
    let axis = if size.x >= size.y && size.x >= size.z {
        0
    } else if size.y >= size.z {
        1
    } else {
        2
    };
    items.sort_by(|&a, &b| centers[a][axis].total_cmp(&centers[b][axis]));
    let mid = items.len() / 2;
    let (left, right) = items.split_at_mut(mid);
    build_tree(ledges, centers, left, out);
    out[me].right = out.len() - me;
    build_tree(ledges, centers, right, out);
}

fn surface_blob(ledges: &[Ledge], hull: Option<&Ledge>, drag: &[f32; 3]) -> Option<Vec<u8>> {
    if ledges.is_empty() {
        return None;
    }
    let hull = hull.filter(|_| ledges.len() > 1);
    let all: Vec<&Ledge> = ledges.iter().chain(hull).collect();
    let mut ledge_at = Vec::with_capacity(all.len());
    let mut at = SURFACE_HEADER;
    for l in &all {
        ledge_at.push(at);
        at += LEDGE_HEADER + l.tris.len() * TRIANGLE;
    }
    let mut points_at = Vec::with_capacity(all.len());
    for l in &all {
        points_at.push(at);
        at += l.points.len() * POINT;
    }
    let tree_at = at;
    let byte_size = tree_at + (2 * ledges.len() - 1) * NODE;
    if byte_size > 0x00FF_FFFF {
        return None;
    }
    let (mut v, mut s1, mut s2) = (0.0, DVec3::ZERO, DVec3::ZERO);
    for l in ledges {
        let (a, b, c) = ledge_moments(l);
        v += a;
        s1 += b;
        s2 += c;
    }
    if v.is_nan() || v <= 1e-15 {
        return None;
    }
    let c = s1 / v;
    let m = (s2 / v - c * c).max(DVec3::ZERO);
    let inertia = DVec3::new(m.y.hypot(m.z), m.x.hypot(m.z), m.x.hypot(m.y));
    let mc = [c.x as f32, c.y as f32, c.z as f32];
    let mcd = DVec3::new(f64::from(mc[0]), f64::from(mc[1]), f64::from(mc[2]));
    let mut ulr = 0.0f64;
    for l in ledges {
        for i in 0..l.points.len() {
            ulr = ulr.max(l.point(i).distance(mcd));
        }
    }
    let ulr = (ulr * (1.0 + 1e-6)) as f32;
    let centers: Vec<DVec3> = ledges
        .iter()
        .map(|l| {
            let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
            for i in 0..l.points.len() {
                lo = lo.min(l.point(i));
                hi = hi.max(l.point(i));
            }
            (lo + hi) * 0.5
        })
        .collect();
    let mut items: Vec<usize> = (0..ledges.len()).collect();
    let mut nodes = Vec::with_capacity(2 * ledges.len());
    build_tree(ledges, &centers, &mut items, &mut nodes);
    if nodes.len() != 2 * ledges.len() - 1 {
        return None;
    }
    let mut b = Buf::default();
    b.b.extend_from_slice(b"VPHY");
    b.i16(0x100);
    b.i16(0);
    b.i32(byte_size as i32);
    for a in drag {
        b.f32(a.max(1e-4));
    }
    b.i32(0);
    for x in mc {
        b.f32(x);
    }
    b.f32(inertia.x as f32);
    b.f32(inertia.y as f32);
    b.f32(inertia.z as f32);
    b.f32(ulr);
    b.u32(250 | ((byte_size as u32) << 8));
    b.i32(tree_at as i32);
    b.u32(0);
    b.u32(0);
    b.u32(IVPS);
    for (k, l) in all.iter().enumerate() {
        let is_hull = k == ledges.len();
        let size16 = (1 + l.tris.len() + l.points.len()) as u32;
        b.i32((points_at[k] - ledge_at[k]) as i32);
        b.i32(if is_hull { tree_at as i32 - ledge_at[k] as i32 } else { 0 });
        b.u32(u32::from(is_hull) | (1 << 2) | (size16 << 8));
        b.i16(l.tris.len() as i16);
        b.i16(0);
        let virt = u32::from(l.is_virtual) << 31;
        for (t, tri) in l.tris.iter().enumerate() {
            let pierce = u32::from(l.pierce.get(t).copied().unwrap_or(0)) & 0xfff;
            b.u32((t as u32 & 0xfff) | (pierce << 12) | virt);
            for (&start, &twin) in tri.iter().zip(&l.twins[t]) {
                b.u32(u32::from(start) | ((twin as u32 & 0x7fff) << 16) | virt);
            }
        }
    }
    for l in &all {
        for p in &l.points {
            b.f32(p[0]);
            b.f32(p[1]);
            b.f32(p[2]);
            b.f32(0.0);
        }
    }
    for (i, n) in nodes.iter().enumerate() {
        let node_at = tree_at + i * NODE;
        if n.right == 0 {
            b.i32(0);
            b.i32(ledge_at[n.ledge] as i32 - node_at as i32);
        } else {
            b.i32((n.right * NODE) as i32);
            b.i32(if i == 0 && hull.is_some() { ledge_at[ledges.len()] as i32 - node_at as i32 } else { 0 });
        }
        for x in n.center {
            b.f32(x);
        }
        b.f32(n.radius);
        b.b.extend_from_slice(&n.box_sizes);
        b.u8(0);
    }
    debug_assert_eq!(b.b.len(), VPHY_HEADER + byte_size);
    Some(b.b)
}
