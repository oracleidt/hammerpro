use glam::{DMat3, DVec2, DVec3};

use crate::mesh::TriMesh;

pub fn hull_2d(points: &[DVec2]) -> Vec<DVec2> {
    let mut p: Vec<DVec2> = points.iter().copied().filter(|v| v.is_finite()).collect();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| (*a - *b).length_squared() < 1e-12);
    if p.len() < 3 {
        return p;
    }
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let mut lower: Vec<DVec2> = Vec::new();
    for &v in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], v) <= 1e-12 {
            lower.pop();
        }
        lower.push(v);
    }
    let mut upper: Vec<DVec2> = Vec::new();
    for &v in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], v) <= 1e-12 {
            upper.pop();
        }
        upper.push(v);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

pub fn polygon_area(poly: &[DVec2]) -> f64 {
    let n = poly.len();
    if n < 3 {
        return 0.0;
    }
    (0..n).map(|i| poly[i].perp_dot(poly[(i + 1) % n])).sum::<f64>().abs() * 0.5
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrientedRect {
    pub center: DVec2,
    pub size: DVec2,
    pub angle: f64,
}

pub fn min_area_rect(hull: &[DVec2]) -> Option<OrientedRect> {
    if hull.is_empty() {
        return None;
    }
    let mut best: Option<(f64, OrientedRect)> = None;
    let n = hull.len();
    let mut dirs: Vec<DVec2> = (0..n).filter_map(|i| (hull[(i + 1) % n] - hull[i]).try_normalize()).collect();
    dirs.push(DVec2::X);
    for u in dirs {
        let v = u.perp();
        let (mut ulo, mut uhi, mut vlo, mut vhi) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for p in hull {
            let (a, b) = (p.dot(u), p.dot(v));
            ulo = ulo.min(a);
            uhi = uhi.max(a);
            vlo = vlo.min(b);
            vhi = vhi.max(b);
        }
        let area = (uhi - ulo) * (vhi - vlo);
        if best.as_ref().is_none_or(|(a, _)| area < *a - 1e-9) {
            let center = u * ((ulo + uhi) * 0.5) + v * ((vlo + vhi) * 0.5);
            let (mut size, mut axis) = (DVec2::new(uhi - ulo, vhi - vlo), u);
            if size.y > size.x {
                size = DVec2::new(size.y, size.x);
                axis = v;
            }
            let mut angle = axis.y.atan2(axis.x).to_degrees();
            if angle <= -90.0 {
                angle += 180.0;
            } else if angle > 90.0 {
                angle -= 180.0;
            }
            best = Some((area, OrientedRect { center, size, angle }));
        }
    }
    best.map(|b| b.1)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrientedBox {
    pub center: DVec3,
    pub axes: [DVec3; 3],
    pub size: DVec3,
}

pub fn oriented_box(mesh: &TriMesh, rotation: DMat3) -> Option<OrientedBox> {
    let axes = [rotation.x_axis, rotation.y_axis, rotation.z_axis].map(|a| a.try_normalize().unwrap_or(a));
    let verts = mesh.used_vertices();
    if verts.is_empty() {
        return None;
    }
    let mut lo = DVec3::INFINITY;
    let mut hi = DVec3::NEG_INFINITY;
    for v in &verts {
        let p = DVec3::new(v.dot(axes[0]), v.dot(axes[1]), v.dot(axes[2]));
        lo = lo.min(p);
        hi = hi.max(p);
    }
    let c = (lo + hi) * 0.5;
    Some(OrientedBox { center: axes[0] * c.x + axes[1] * c.y + axes[2] * c.z, axes, size: hi - lo })
}

pub fn footprint(mesh: &TriMesh) -> Vec<DVec2> {
    let pts: Vec<DVec2> = mesh.used_vertices().iter().map(|v| v.truncate()).collect();
    hull_2d(&pts)
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContactPatch {
    pub z: f64,
    pub outline: Vec<DVec2>,
    pub area: f64,
    pub center: DVec2,
    pub points: usize,
}

pub fn contact_patch(mesh: &TriMesh, eps: f64) -> Option<ContactPatch> {
    let verts = mesh.used_vertices();
    let z = verts.iter().map(|v| v.z).fold(f64::INFINITY, f64::min);
    if !z.is_finite() {
        return None;
    }
    let low: Vec<DVec2> = verts.iter().filter(|v| v.z <= z + eps).map(|v| v.truncate()).collect();
    let outline = hull_2d(&low);
    let center = if low.is_empty() { DVec2::ZERO } else { low.iter().copied().sum::<DVec2>() / low.len() as f64 };
    Some(ContactPatch { z, area: polygon_area(&outline), outline, center, points: low.len() })
}

#[derive(Clone, Debug, PartialEq)]
pub struct HeightMap {
    pub min: DVec2,
    pub cell: f64,
    pub nx: usize,
    pub ny: usize,
    pub top: Vec<Option<f64>>,
    pub bottom: Vec<Option<f64>>,
}

pub fn height_map(mesh: &TriMesh, cells: usize) -> Option<HeightMap> {
    let b = mesh.bounds();
    if b.is_empty() {
        return None;
    }
    let size = (b.max - b.min).truncate().max(DVec2::splat(1e-3));
    let cells = cells.clamp(1, 64);
    let cell = size.x.max(size.y) / cells as f64;
    let nx = ((size.x / cell).ceil() as usize).clamp(1, 64);
    let ny = ((size.y / cell).ceil() as usize).clamp(1, 64);
    let min = b.min.truncate();
    let mut top = vec![None::<f64>; nx * ny];
    let mut bottom = vec![None::<f64>; nx * ny];
    for i in 0..mesh.tris.len() {
        let t = mesh.tri(i);
        let p = t.map(|v| v.truncate());
        let lo = p[0].min(p[1]).min(p[2]);
        let hi = p[0].max(p[1]).max(p[2]);
        let x0 = (((lo.x - min.x) / cell - 0.5).ceil().max(0.0)) as usize;
        let y0 = (((lo.y - min.y) / cell - 0.5).ceil().max(0.0)) as usize;
        let x1 = ((((hi.x - min.x) / cell - 0.5).floor()) as i64).min(nx as i64 - 1);
        let y1 = ((((hi.y - min.y) / cell - 0.5).floor()) as i64).min(ny as i64 - 1);
        if x1 < 0 || y1 < 0 {
            continue;
        }
        for y in y0..=(y1 as usize) {
            for x in x0..=(x1 as usize) {
                let q = min + DVec2::new((x as f64 + 0.5) * cell, (y as f64 + 0.5) * cell);
                let Some((u, v, w)) = crate::grid::bary(q, p[0], p[1], p[2]) else { continue };
                let z = u * t[0].z + v * t[1].z + w * t[2].z;
                let k = y * nx + x;
                top[k] = Some(top[k].map_or(z, |o: f64| o.max(z)));
                bottom[k] = Some(bottom[k].map_or(z, |o: f64| o.min(z)));
            }
        }
    }
    Some(HeightMap { min, cell, nx, ny, top, bottom })
}

#[derive(Clone, Debug, PartialEq)]
pub struct Slice {
    pub z: f64,
    pub outline: Vec<DVec2>,
    pub min: DVec2,
    pub max: DVec2,
    pub area: f64,
}

pub fn slices(mesh: &TriMesh, count: usize) -> Vec<Slice> {
    let b = mesh.bounds();
    if b.is_empty() || count == 0 {
        return Vec::new();
    }
    let h = b.max.z - b.min.z;
    let mut out = Vec::with_capacity(count);
    for s in 0..count {
        let z = b.min.z + h * (s as f64 + 0.5) / count as f64;
        let mut pts: Vec<DVec2> = Vec::new();
        for i in 0..mesh.tris.len() {
            let t = mesh.tri(i);
            for k in 0..3 {
                let (a, c) = (t[k], t[(k + 1) % 3]);
                if (a.z - z) * (c.z - z) < 0.0 {
                    let f = (z - a.z) / (c.z - a.z);
                    pts.push(a.lerp(c, f).truncate());
                } else if (a.z - z).abs() < 1e-9 {
                    pts.push(a.truncate());
                }
            }
        }
        let outline = hull_2d(&pts);
        let (lo, hi) = pts.iter().fold((DVec2::INFINITY, DVec2::NEG_INFINITY), |(l, h), p| (l.min(*p), h.max(*p)));
        if outline.is_empty() {
            out.push(Slice { z, outline, min: DVec2::ZERO, max: DVec2::ZERO, area: 0.0 });
        } else {
            out.push(Slice { z, area: polygon_area(&outline), outline, min: lo, max: hi });
        }
    }
    out
}
