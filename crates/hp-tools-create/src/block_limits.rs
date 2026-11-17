use glam::{DVec2, DVec3};
use hp_app::state::AppState;
use hp_editor::{Id, PickOptions};
use hp_geom::{Aabb, Ray};

pub(crate) const LIMIT_EPS: f64 = 0.01;
pub(crate) const MIN_HEIGHT: f64 = 0.125;
const SAMPLES: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Footprint {
    pub axis: usize,
    pub level: f64,
    pub lo: DVec2,
    pub hi: DVec2,
}

pub(crate) fn plane_axes(axis: usize) -> (usize, usize) {
    ((axis + 1) % 3, (axis + 2) % 3)
}

impl Footprint {
    pub(crate) fn point(&self, u: f64, v: f64, d: f64) -> DVec3 {
        let (i, j) = plane_axes(self.axis);
        let mut p = DVec3::ZERO;
        p[self.axis] = self.level + d;
        p[i] = u;
        p[j] = v;
        p
    }

    fn inset(&self) -> (DVec2, DVec2) {
        let lo = self.lo + DVec2::splat(LIMIT_EPS);
        let hi = self.hi - DVec2::splat(LIMIT_EPS);
        (lo.min(hi), lo.max(hi))
    }

    fn prism(&self, dir: f64, max: f64) -> Aabb {
        let a = self.point(self.lo.x, self.lo.y, 0.0);
        let b = self.point(self.hi.x, self.hi.y, dir * max);
        Aabb { min: a.min(b), max: a.max(b) }.expand(LIMIT_EPS)
    }
}

pub(crate) fn contact_distance(state: &AppState, fp: &Footprint, dir: f64, max: f64) -> Option<f64> {
    if !(max.is_finite() && max > 0.0) {
        return None;
    }
    let (lo, hi) = fp.inset();
    let mut axis_dir = DVec3::ZERO;
    axis_dir[fp.axis] = dir;
    let lift = 2.0 * LIMIT_EPS;
    let opts = PickOptions {
        include_solids: true,
        include_entities: false,
        ignore_hidden: true,
        max_distance: max + 1e-6,
        ..PickOptions::default()
    };
    let mut best: Option<f64> = None;
    let steps = (SAMPLES - 1) as f64;
    for a in 0..SAMPLES {
        for b in 0..SAMPLES {
            let u = lo.x + (hi.x - lo.x) * a as f64 / steps;
            let v = lo.y + (hi.y - lo.y) * b as f64 / steps;
            let origin = fp.point(u, v, dir * lift);
            let ray = Ray::new(origin, axis_dir);
            if let Some(hit) = state.doc.pick_ray(&ray, &opts, &|e| state.entity_bounds(e)) {
                let t = hit.t + lift;
                if t.is_finite() && t <= max && best.is_none_or(|b| t < b) {
                    best = Some(t);
                }
            }
        }
    }
    best
}

fn clip(poly: Vec<DVec3>, f: &dyn Fn(DVec3) -> f64) -> Vec<DVec3> {
    let n = poly.len();
    let mut out = Vec::with_capacity(n + 2);
    for k in 0..n {
        let (a, b) = (poly[k], poly[(k + 1) % n]);
        let (fa, fb) = (f(a), f(b));
        if fa >= 0.0 {
            out.push(a);
        }
        if (fa >= 0.0) != (fb >= 0.0) {
            let t = fa / (fa - fb);
            out.push(a + (b - a) * t);
        }
    }
    out
}

pub(crate) fn solid_contact(geom: &hp_geom::SolidGeom, fp: &Footprint, dir: f64) -> Option<f64> {
    let (i, j) = plane_axes(fp.axis);
    let (lo, hi) = fp.inset();
    let a = fp.axis;
    let level = fp.level;
    let depth = move |p: DVec3| dir * (p[a] - level);
    let planes: [&dyn Fn(DVec3) -> f64; 5] = [
        &|p: DVec3| p[i] - lo.x,
        &|p: DVec3| hi.x - p[i],
        &|p: DVec3| p[j] - lo.y,
        &|p: DVec3| hi.y - p[j],
        &|p: DVec3| depth(p) - LIMIT_EPS,
    ];
    let mut best: Option<f64> = None;
    let mut keep = |d: f64| {
        if d.is_finite() && best.is_none_or(|b| d < b) {
            best = Some(d);
        }
    };
    for face in &geom.faces {
        let mut poly = face.vertices.clone();
        for f in planes {
            if poly.len() < 3 {
                break;
            }
            poly = clip(poly, f);
        }
        if poly.len() >= 3 {
            for p in &poly {
                keep(depth(*p));
            }
        }
    }
    for (u, v) in [(lo.x, lo.y), (hi.x, lo.y), (lo.x, hi.y), (hi.x, hi.y)] {
        let p = fp.point(u, v, dir * LIMIT_EPS);
        if !geom.faces.is_empty() && geom.faces.iter().all(|f| f.plane.distance(p) < -1e-9) {
            keep(LIMIT_EPS);
        }
    }
    best
}

pub(crate) fn overlap_distance(state: &AppState, fp: &Footprint, dir: f64, max: f64) -> Option<f64> {
    if !(max.is_finite() && max > 0.0) {
        return None;
    }
    let mut best: Option<f64> = None;
    for id in state.doc.solids_in_box(&fp.prism(dir, max)) {
        if !state.doc.is_solid_pickable(id) {
            continue;
        }
        let Some(node) = state.doc.scene().solid(id) else { continue };
        if let Some(d) = solid_contact(&node.geom, fp, dir)
            && d < max - 1e-6
            && best.is_none_or(|b| d < b)
        {
            best = Some(d);
        }
    }
    best
}

pub(crate) fn face_polygon(state: &AppState, solid: Id, side: usize) -> Option<Vec<DVec3>> {
    let node = state.doc.scene().solid(solid)?;
    let face = node.geom.face_for_side(side)?;
    (face.vertices.len() >= 3).then(|| face.vertices.clone())
}

pub(crate) fn project(poly: &[DVec3], axis: usize) -> Vec<DVec2> {
    let (i, j) = plane_axes(axis);
    poly.iter().map(|p| DVec2::new(p[i], p[j])).collect()
}

struct Convex {
    edges: Vec<(DVec2, DVec2)>,
}

impl Convex {
    fn new(poly: &[DVec2]) -> Option<Self> {
        let n = poly.len();
        if n < 3 {
            return None;
        }
        let area: f64 = (0..n).map(|k| poly[k].perp_dot(poly[(k + 1) % n])).sum::<f64>() * 0.5;
        if area.is_nan() || area.abs() < 1.0 {
            return None;
        }
        let s = area.signum();
        let edges = (0..n)
            .filter_map(|k| {
                let (p, q) = (poly[k], poly[(k + 1) % n]);
                let e = q - p;
                let len = e.length();
                (len > 1e-9).then(|| (p, DVec2::new(-e.y, e.x) * (s / len)))
            })
            .collect();
        Some(Self { edges })
    }

    fn inside(&self, x: DVec2) -> bool {
        self.edges.iter().all(|(p, n)| n.dot(x - *p) >= -1e-6)
    }

    fn reach(&self, s: DVec2, d: DVec2) -> f64 {
        let mut t: f64 = 1.0;
        for (p, n) in &self.edges {
            let nd = n.dot(d);
            if nd < -1e-12 {
                t = t.min(n.dot(s - *p).max(0.0) / -nd);
            }
        }
        t.clamp(0.0, 1.0)
    }

    fn nearest(&self, x: DVec2) -> DVec2 {
        if self.inside(x) {
            return x;
        }
        let mut best = x;
        let mut dist = f64::INFINITY;
        for (k, (p, _)) in self.edges.iter().enumerate() {
            let q = self.edges[(k + 1) % self.edges.len()].0;
            let e = q - *p;
            let t = ((x - *p).dot(e) / e.length_squared().max(1e-18)).clamp(0.0, 1.0);
            let c = *p + e * t;
            let d = c.distance_squared(x);
            if d < dist {
                dist = d;
                best = c;
            }
        }
        best
    }
}

pub(crate) fn clamp_start(poly: &[DVec2], s: DVec2) -> Option<DVec2> {
    Convex::new(poly).map(|c| c.nearest(s))
}

pub(crate) fn clamp_rect(poly: &[DVec2], s: DVec2, e: DVec2) -> Option<(DVec2, DVec2)> {
    let c = Convex::new(poly)?;
    let s = c.nearest(s);
    let d = e - s;
    let tx = c.reach(s, DVec2::new(d.x, 0.0));
    let ty = c.reach(s, DVec2::new(0.0, d.y));
    let corner = DVec2::new(d.x * tx, d.y * ty);
    let k = c.reach(s, corner);
    Some((s, s + corner * k))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(min: DVec3, max: DVec3) -> hp_geom::SolidGeom {
        let mut next = 1u32;
        let params = hp_geom::PrimitiveParams {
            kind: hp_geom::Primitive::Block,
            bounds: Aabb { min, max },
            material: "dev".into(),
            ..hp_geom::PrimitiveParams::default()
        };
        let solids = hp_geom::make_primitive(&params, &mut || {
            next += 1;
            next
        })
        .expect("block");
        hp_geom::build_solid(&solids[0])
    }

    #[test]
    fn exact_contact_finds_a_thin_brush_between_the_rays() {
        let fp = Footprint { axis: 2, level: 0.0, lo: DVec2::new(0.0, 0.0), hi: DVec2::new(64.0, 64.0) };
        let pole = cube(DVec3::new(10.0, 10.0, 40.0), DVec3::new(11.0, 11.0, 80.0));
        let d = solid_contact(&pole, &fp, 1.0).expect("the pole is inside the prism");
        assert!((d - 40.0).abs() < 1e-6, "{d}");
        assert!(solid_contact(&pole, &fp, -1.0).is_none(), "nothing below the base");
    }

    #[test]
    fn touching_neighbours_do_not_count_and_a_crossing_brush_blocks_at_once() {
        let fp = Footprint { axis: 2, level: 0.0, lo: DVec2::new(0.0, 0.0), hi: DVec2::new(64.0, 64.0) };
        let side = cube(DVec3::new(64.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 64.0));
        assert!(solid_contact(&side, &fp, 1.0).is_none());
        let floor = cube(DVec3::new(-64.0, -64.0, -16.0), DVec3::new(128.0, 128.0, 0.0));
        assert!(solid_contact(&floor, &fp, 1.0).is_none());
        let through = cube(DVec3::new(-64.0, -64.0, -16.0), DVec3::new(128.0, 128.0, 16.0));
        let d = solid_contact(&through, &fp, 1.0).expect("the base is inside");
        assert!(d <= LIMIT_EPS + 1e-9);
        let beam = cube(DVec3::new(30.0, -10.0, 100.0), DVec3::new(34.0, 80.0, 200.0));
        assert!((solid_contact(&beam, &fp, 1.0).expect("wall") - 100.0).abs() < 1e-6);
    }

    #[test]
    fn footprint_stays_inside_a_rotated_face() {
        let diamond = [DVec2::new(0.0, -32.0), DVec2::new(32.0, 0.0), DVec2::new(0.0, 32.0), DVec2::new(-32.0, 0.0)];
        let c = Convex::new(&diamond).expect("convex");
        for e in [DVec2::new(100.0, 100.0), DVec2::new(-50.0, 7.0), DVec2::new(3.0, -400.0), DVec2::new(10.0, 5.0)] {
            let (s, end) = clamp_rect(&diamond, DVec2::ZERO, e).expect("clamped");
            for p in [s, end, DVec2::new(s.x, end.y), DVec2::new(end.x, s.y)] {
                assert!(c.inside(p), "{p:?} for {e:?}");
            }
        }
        let (_, inside) = clamp_rect(&diamond, DVec2::ZERO, DVec2::new(10.0, 5.0)).expect("inside");
        assert_eq!(inside, DVec2::new(10.0, 5.0), "a footprint inside the face is not changed");
        let (s, _) = clamp_rect(&diamond, DVec2::new(40.0, 40.0), DVec2::new(0.0, 0.0)).expect("start");
        assert!(c.inside(s));
        assert!(clamp_rect(&[DVec2::ZERO, DVec2::X, DVec2::X * 2.0], DVec2::ZERO, DVec2::ONE).is_none());
    }
}
