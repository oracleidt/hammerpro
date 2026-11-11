use glam::{DVec2, DVec3};
use hp_geom::Aabb;

use crate::grid::{Grid2, Stamp};
use crate::mesh::TriMesh;
use crate::sweep::sweep;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Overlap {
    pub crossings: usize,
    pub point: DVec3,
    pub contained: bool,
}

fn tri_bounds(t: &[DVec3; 3]) -> Aabb {
    Aabb::from_points(t.iter().copied())
}

fn inside_tri(p: DVec3, t: &[DVec3; 3], n: DVec3) -> bool {
    let e = -1e-9;
    (0..3).all(|k| {
        let a = t[k];
        let b = t[(k + 1) % 3];
        (b - a).cross(p - a).dot(n) >= e * (b - a).length()
    })
}

fn edge_cross(a: DVec3, b: DVec3, t: &[DVec3; 3], n: DVec3, tol: f64) -> Option<DVec3> {
    let da = (a - t[0]).dot(n);
    let db = (b - t[0]).dot(n);
    if !((da > tol && db < -tol) || (da < -tol && db > tol)) {
        return None;
    }
    let p = a + (b - a) * (da / (da - db));
    inside_tri(p, t, n).then_some(p)
}

fn tri_cross(x: &[DVec3; 3], nx: DVec3, y: &[DVec3; 3], ny: DVec3, tol: f64) -> Option<DVec3> {
    for k in 0..3 {
        if let Some(p) = edge_cross(x[k], x[(k + 1) % 3], y, ny, tol) {
            return Some(p);
        }
        if let Some(p) = edge_cross(y[k], y[(k + 1) % 3], x, nx, tol) {
            return Some(p);
        }
    }
    None
}

pub fn contains_point(mesh: &TriMesh, p: DVec3) -> bool {
    let b = mesh.bounds();
    if b.is_empty() || !b.contains_point(p) {
        return false;
    }
    let dir = DVec3::new(0.1234, 0.2468, 1.0).normalize();
    let ray = hp_geom::Ray::new(p, dir);
    let mut n = 0;
    for i in 0..mesh.tris.len() {
        let [a, bb, c] = mesh.tri(i);
        if let Some(t) = hp_geom::ray_triangle(&ray, a, bb, c, false)
            && t > 1e-9
        {
            n += 1;
        }
    }
    n % 2 == 1
}

pub fn overlap(a: &TriMesh, b: &TriMesh, tol: f64) -> Option<Overlap> {
    let (ba, bb) = (a.bounds(), b.bounds());
    if ba.is_empty() || bb.is_empty() || !ba.expand(-tol).intersects(&bb.expand(-tol)) {
        return None;
    }
    let region = ba.intersection(&bb).expand(tol);
    let ib: Vec<usize> = (0..b.tris.len()).filter(|&i| tri_bounds(&b.tri(i)).intersects(&region)).collect();
    let ia: Vec<usize> = (0..a.tris.len()).filter(|&i| tri_bounds(&a.tri(i)).intersects(&region)).collect();
    let mut crossings = 0usize;
    let mut sum = DVec3::ZERO;
    if !ib.is_empty() && !ia.is_empty() {
        let lo = DVec2::new(region.min.x, region.min.y);
        let hi = DVec2::new(region.max.x, region.max.y);
        let mut grid = Grid2::new(lo, hi, ib.len());
        for (k, &i) in ib.iter().enumerate() {
            let tb = tri_bounds(&b.tri(i));
            grid.insert(k as u32, DVec2::new(tb.min.x, tb.min.y), DVec2::new(tb.max.x, tb.max.y));
        }
        let mut stamp = Stamp::new(ib.len());
        for &i in &ia {
            let ta = a.tri(i);
            let bx = tri_bounds(&ta);
            grid.visit(DVec2::new(bx.min.x, bx.min.y), DVec2::new(bx.max.x, bx.max.y), &mut stamp, |k| {
                let j = ib[k as usize];
                let tb = b.tri(j);
                if !tri_bounds(&tb).intersects(&bx) {
                    return;
                }
                if let Some(p) = tri_cross(&ta, a.normals[i], &tb, b.normals[j], tol) {
                    crossings += 1;
                    sum += p;
                }
            });
        }
    }
    if crossings > 0 {
        return Some(Overlap { crossings, point: sum / crossings as f64, contained: false });
    }
    let probe = |m: &TriMesh| m.tris.first().map(|t| m.verts[t[0] as usize]);
    if b.closed
        && let Some(p) = probe(a)
        && contains_point(b, p)
    {
        return Some(Overlap { crossings: 0, point: p, contained: true });
    }
    if a.closed
        && let Some(p) = probe(b)
        && contains_point(a, p)
    {
        return Some(Overlap { crossings: 0, point: p, contained: true });
    }
    None
}

pub fn separation(a: &TriMesh, b: &TriMesh, dir: DVec3) -> f64 {
    let Some(d) = dir.try_normalize() else { return 0.0 };
    let (ba, bb) = (a.bounds(), b.bounds());
    if ba.is_empty() || bb.is_empty() {
        return 0.0;
    }
    let lowest = ba.corners().iter().map(|c| c.dot(d)).fold(f64::INFINITY, f64::min);
    let highest = bb.corners().iter().map(|c| c.dot(d)).fold(f64::NEG_INFINITY, f64::max);
    let l = (highest - lowest).max(0.0) + 1.0;
    match sweep(a, d * l, &[b], -d, 0.0, l + 1e-6) {
        Some(c) => (l - c.t).max(0.0),
        None => 0.0,
    }
}

fn tri_box(t: &[DVec3; 3], b: &Aabb) -> bool {
    let c = b.center();
    let h = b.size() * 0.5;
    let v = t.map(|p| p - c);
    let e = [v[1] - v[0], v[2] - v[1], v[0] - v[2]];
    let axes_hit = |axis: DVec3| {
        if axis.length_squared() < 1e-18 {
            return true;
        }
        let p = v.map(|x| x.dot(axis));
        let r = h.x * axis.x.abs() + h.y * axis.y.abs() + h.z * axis.z.abs();
        let (lo, hi) = (p[0].min(p[1]).min(p[2]), p[0].max(p[1]).max(p[2]));
        !(lo > r || hi < -r)
    };
    for ei in e {
        for a in [DVec3::X, DVec3::Y, DVec3::Z] {
            if !axes_hit(a.cross(ei)) {
                return false;
            }
        }
    }
    for a in [DVec3::X, DVec3::Y, DVec3::Z] {
        if !axes_hit(a) {
            return false;
        }
    }
    axes_hit(e[0].cross(e[1]))
}

pub fn touches_box(mesh: &TriMesh, b: &Aabb) -> bool {
    let mb = mesh.bounds();
    if mb.is_empty() || !mb.intersects(b) {
        return false;
    }
    if b.contains_aabb(&mb) {
        return true;
    }
    for i in 0..mesh.tris.len() {
        let t = mesh.tri(i);
        if tri_bounds(&t).intersects(b) && tri_box(&t, b) {
            return true;
        }
    }
    mesh.closed && contains_point(mesh, b.center())
}
