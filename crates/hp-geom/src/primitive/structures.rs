use glam::{DVec2, DVec3};

use super::{Primitive, PrimitiveParams, ellipse_point, extrude, strictly_convex};
use crate::{Aabb, GeomError, WELD_EPS};

pub const MAX_STAIR_STEPS: u32 = 256;
pub const MAX_STAIR_ANGLE: f64 = 3600.0;

fn invalid(message: String) -> GeomError {
    GeomError::InvalidInput(message)
}

fn levels(from: f64, to: f64, count: u32, round: bool) -> Vec<f64> {
    let exact: Vec<f64> = (0..=count).map(|k| from + (to - from) * f64::from(k) / f64::from(count)).collect();
    if round {
        let snapped: Vec<f64> = exact.iter().map(|v| v.round() + 0.0).collect();
        if snapped.windows(2).all(|w| w[1] - w[0] >= 1.0) {
            return snapped;
        }
    }
    exact
}

pub fn step_count(total: f64, steps: u32, step_height: f64) -> Result<u32, GeomError> {
    if !step_height.is_finite() || step_height < 0.0 {
        return Err(invalid(format!("invalid step height {step_height}")));
    }
    let count = if step_height > 0.0 { (total / step_height).round().clamp(1.0, 1e6) as u32 } else { steps };
    if count == 0 || count > MAX_STAIR_STEPS {
        return Err(invalid(format!("steps must be between 1 and {MAX_STAIR_STEPS}, got {count}")));
    }
    if total / f64::from(count) < 0.5 {
        return Err(invalid("the steps would be lower than half a unit".to_owned()));
    }
    Ok(count)
}

fn slab_bottom(rises: &[f64], index: usize, open: bool) -> f64 {
    if open { rises[index.saturating_sub(1)] } else { rises[0] }
}

fn cuboid(x0: f64, x1: f64, y0: f64, y1: f64, z0: f64, z1: f64) -> Vec<DVec3> {
    let v = DVec3::new;
    vec![
        v(x0, y0, z0),
        v(x1, y0, z0),
        v(x0, y1, z0),
        v(x1, y1, z0),
        v(x0, y0, z1),
        v(x1, y0, z1),
        v(x0, y1, z1),
        v(x1, y1, z1),
    ]
}

fn stairs(p: &PrimitiveParams, b: &Aabb) -> Result<Vec<Vec<DVec3>>, GeomError> {
    let n = step_count(b.max.z - b.min.z, p.stair_steps, p.stair_step_height)?;
    if (b.max.y - b.min.y) / f64::from(n) < 0.5 {
        return Err(invalid("the steps would be shallower than half a unit".to_owned()));
    }
    let runs = levels(b.min.y, b.max.y, n, p.round_vertices);
    let rises = levels(b.min.z, b.max.z, n, p.round_vertices);
    Ok((0..n as usize)
        .map(|i| cuboid(b.min.x, b.max.x, runs[i], runs[i + 1], slab_bottom(&rises, i, p.stair_open), rises[i + 1]))
        .collect())
}

fn spiral(p: &PrimitiveParams, b: &Aabb) -> Result<Vec<Vec<DVec3>>, GeomError> {
    let n = step_count(b.max.z - b.min.z, p.stair_steps, p.stair_step_height)?;
    let angle = p.spiral_angle;
    if !angle.is_finite() || angle == 0.0 || angle.abs() > MAX_STAIR_ANGLE {
        return Err(invalid(format!("spiral angle must be between -{MAX_STAIR_ANGLE} and {MAX_STAIR_ANGLE}, not 0")));
    }
    if angle.abs() / f64::from(n) >= 180.0 - 1e-9 {
        return Err(invalid("each step must cover less than 180 degrees".to_owned()));
    }
    if !p.stair_open && angle.abs() > 360.0 + 1e-9 {
        return Err(invalid("solid steps cannot turn more than 360 degrees (use open steps)".to_owned()));
    }
    let base = if p.spiral_start.is_finite() { p.spiral_start } else { 0.0 };
    let start = base + p.direction.resolve(b.size()).start_angle();
    let center = DVec2::new((b.min.x + b.max.x) * 0.5, (b.min.y + b.max.y) * 0.5);
    let (rx, ry) = ((b.max.x - b.min.x) * 0.5, (b.max.y - b.min.y) * 0.5);
    let rmin = rx.min(ry);
    let inner = p.spiral_inner;
    if !inner.is_finite() || inner < 0.0 || inner >= rmin - 1.0 {
        return Err(invalid(format!("inner radius must be between 0 and {}, got {inner}", rmin - 1.0)));
    }
    let f = inner / rmin;
    let pie = inner <= WELD_EPS;
    let full = (angle.abs() - 360.0).abs() < 1e-9;
    let make = |round: bool| -> Vec<[DVec2; 4]> {
        let pt = |s: f64, k: u32| {
            let k = if full && k == n { 0 } else { k };
            let q = ellipse_point(center, rx * s, ry * s, start + angle * f64::from(k) / f64::from(n));
            if round { q.round() + DVec2::ZERO } else { q }
        };
        (0..n)
            .map(|k| {
                let (ia, ib) = if pie { (center, center) } else { (pt(f, k), pt(f, k + 1)) };
                [pt(1.0, k), pt(1.0, k + 1), ib, ia]
            })
            .collect()
    };
    let quad_ok = |q: &[DVec2; 4]| {
        let mut poly: Vec<DVec2> = q.to_vec();
        poly.dedup_by(|a, b| a.distance_squared(*b) < 1e-12);
        strictly_convex(&poly)
    };
    let mut quads = make(false);
    if p.round_vertices {
        let r = make(true);
        if r.iter().all(quad_ok) {
            quads = r;
        }
    }
    let rises = levels(b.min.z, b.max.z, n, p.round_vertices);
    Ok(quads.iter().enumerate().map(|(i, q)| extrude(q, slab_bottom(&rises, i, p.stair_open), rises[i + 1])).collect())
}

fn ramp(p: &PrimitiveParams, b: &Aabb) -> Result<Vec<Vec<DVec3>>, GeomError> {
    let a = p.ramp_angle;
    if !a.is_finite() || a <= 0.0 || a >= 90.0 {
        return Err(invalid(format!("slope angle must be between 0 and 90 degrees, got {a}")));
    }
    let t = a.to_radians().tan();
    let (run, height) = (b.max.y - b.min.y, b.max.z - b.min.z);
    let snap = |v: f64| if p.round_vertices { v.round() } else { v };
    let (mut top, mut slope_end) = (b.min.z + run * t, b.max.y);
    if run * t >= height - 1e-9 {
        top = b.max.z;
        slope_end = b.min.y + height / t;
    }
    let (rt, rs) = (snap(top), snap(slope_end));
    if rt > b.min.z && rt <= b.max.z {
        top = rt;
    }
    if rs > b.min.y && rs <= b.max.y {
        slope_end = rs;
    }
    if top - b.min.z < 0.5 || slope_end - b.min.y < 0.5 {
        return Err(invalid("the ramp would be flatter than half a unit".to_owned()));
    }
    let mut pts = Vec::new();
    for x in [b.min.x, b.max.x] {
        pts.push(DVec3::new(x, b.min.y, b.min.z));
        pts.push(DVec3::new(x, b.max.y, b.min.z));
        pts.push(DVec3::new(x, b.max.y, top));
        if slope_end < b.max.y - 1e-9 {
            pts.push(DVec3::new(x, slope_end, top));
        }
    }
    Ok(vec![pts])
}

fn pillar(p: &PrimitiveParams, b: &Aabb) -> Result<Vec<Vec<DVec3>>, GeomError> {
    let center = DVec2::new((b.min.x + b.max.x) * 0.5, (b.min.y + b.max.y) * 0.5);
    let (rx, ry) = ((b.max.x - b.min.x) * 0.5, (b.max.y - b.min.y) * 0.5);
    let (base, cap, inset) = (p.pillar_base, p.pillar_capital, p.pillar_inset);
    if ![base, cap, inset].iter().all(|v| v.is_finite() && *v >= 0.0) {
        return Err(invalid("pillar base, capital and inset must be zero or more".to_owned()));
    }
    let snap = |v: f64| if p.round_vertices { v.round() } else { v };
    let (base, cap) = (snap(base), snap(cap));
    let (zb, zc) = (b.min.z + base, b.max.z - cap);
    if zc - zb < 1.0 {
        return Err(invalid("the base and the capital leave no room for the shaft".to_owned()));
    }
    let inset = if base > 0.0 || cap > 0.0 { inset } else { 0.0 };
    if inset >= rx.min(ry) - 1.0 {
        return Err(invalid(format!("inset must stay under {}, got {inset}", rx.min(ry) - 1.0)));
    }
    let polygon = |rx: f64, ry: f64| {
        let exact = super::ring(center, rx, ry, p.sides, 0.0);
        if p.round_vertices {
            let r = super::rounded(&exact);
            if strictly_convex(&r) {
                return r;
            }
        }
        exact
    };
    let full = polygon(rx, ry);
    let shaft = polygon(rx - inset, ry - inset);
    let mut sets = Vec::new();
    if base > 0.0 {
        sets.push(extrude(&full, b.min.z, zb));
    }
    sets.push(extrude(&shaft, zb, zc));
    if cap > 0.0 {
        sets.push(extrude(&full, zc, b.max.z));
    }
    Ok(sets)
}

fn doorway(p: &PrimitiveParams, b: &Aabb) -> Result<Vec<Vec<DVec3>>, GeomError> {
    let snap = |v: f64| if p.round_vertices { v.round() } else { v };
    let size = b.size();
    let auto = |v: f64, whole: f64, share: f64| if v == 0.0 { whole * share } else { v };
    let w = snap(auto(p.door_width, size.x, 0.5));
    let h = snap(auto(p.door_height, size.z, 0.75));
    let t = p.door_thickness;
    if !w.is_finite() || !h.is_finite() || !t.is_finite() || w < 1.0 || h < 1.0 || t < 0.0 {
        return Err(invalid("door width and height must be at least 1, thickness 0 or more".to_owned()));
    }
    if w > size.x - 2.0 || h > size.z - 1.0 {
        return Err(invalid(format!(
            "the opening {w} x {h} needs a wall wider than {} and higher than {}",
            w + 2.0,
            h + 1.0
        )));
    }
    let (ya, yb) = if t > 0.0 && t < size.y {
        let ya = snap((b.min.y + b.max.y - t) * 0.5);
        (ya, ya + snap(t).max(1.0))
    } else {
        (b.min.y, b.max.y)
    };
    let ox0 = snap((b.min.x + b.max.x - w) * 0.5);
    let ox1 = ox0 + w;
    let oz = b.min.z + h;
    if ox0 < b.min.x + 1.0 - 1e-9 || ox1 > b.max.x - 1.0 + 1e-9 || yb > b.max.y + 1e-9 {
        return Err(invalid("the opening does not fit the wall".to_owned()));
    }
    let mut sets = vec![cuboid(b.min.x, ox0, ya, yb, b.min.z, b.max.z), cuboid(ox1, b.max.x, ya, yb, b.min.z, b.max.z)];
    if p.door_arch {
        sets.extend(lintel_arch(p, (ox0, ox1), (ya, yb), (b.min.z, oz, b.max.z))?);
    } else {
        sets.push(cuboid(ox0, ox1, ya, yb, oz, b.max.z));
    }
    Ok(sets)
}

pub const MAX_DOOR_ARCH_SEGMENTS: u32 = 64;

fn lintel_arch(
    p: &PrimitiveParams,
    (ox0, ox1): (f64, f64),
    (ya, yb): (f64, f64),
    (floor, crown, top): (f64, f64, f64),
) -> Result<Vec<Vec<DVec3>>, GeomError> {
    let n = p.door_arch_segments;
    if !(2..=MAX_DOOR_ARCH_SEGMENTS).contains(&n) {
        return Err(invalid(format!("arch segments must be between 2 and {MAX_DOOR_ARCH_SEGMENTS}, got {n}")));
    }
    let half = (ox1 - ox0) * 0.5;
    let radius = if p.door_arch_radius == 0.0 { half } else { p.door_arch_radius };
    if !radius.is_finite() || radius < half - 1e-9 {
        return Err(invalid(format!("arch radius must be at least half the opening ({half}), got {radius}")));
    }
    let cx = (ox0 + ox1) * 0.5;
    let cz = crown - radius;
    let spring = cz + (radius * radius - half * half).max(0.0).sqrt();
    if spring < floor - 1e-9 {
        return Err(invalid("the arch radius is too large for the opening height".to_owned()));
    }
    let limit = (half / radius).clamp(-1.0, 1.0).asin();
    let build = |round: bool| -> Vec<(f64, f64)> {
        (0..=n)
            .map(|k| {
                let a = -limit + 2.0 * limit * f64::from(k) / f64::from(n);
                let x = if k == 0 {
                    ox0
                } else if k == n {
                    ox1
                } else {
                    cx + radius * a.sin()
                };
                let z = cz + radius * a.cos();
                if round && k != 0 && k != n { (x.round(), z.round()) } else { (x, z) }
            })
            .collect()
    };
    let valid = |pts: &[(f64, f64)]| {
        pts.windows(2).all(|w| w[1].0 - w[0].0 > 1e-9) && pts.iter().all(|q| q.1 + 0.5 <= top && q.1 >= spring - 1e-9)
    };
    let mut pts = build(false);
    if p.round_vertices {
        let r = build(true);
        if valid(&r) {
            pts = r;
        }
    }
    if !valid(&pts) {
        return Err(invalid("too many arch segments for the opening".to_owned()));
    }
    Ok(pts
        .windows(2)
        .map(|w| {
            let mut set = Vec::with_capacity(8);
            for y in [ya, yb] {
                set.push(DVec3::new(w[0].0, y, w[0].1));
                set.push(DVec3::new(w[1].0, y, w[1].1));
                set.push(DVec3::new(w[0].0, y, top));
                set.push(DVec3::new(w[1].0, y, top));
            }
            set
        })
        .collect())
}

type PointSets = Vec<Vec<DVec3>>;

fn oriented(
    p: &PrimitiveParams,
    b: &Aabb,
    build: fn(&PrimitiveParams, &Aabb) -> Result<PointSets, GeomError>,
) -> Result<PointSets, GeomError> {
    let dir = p.direction.resolve(b.size());
    let mut sets = build(p, &dir.generation_box(b))?;
    dir.map_points(b, &mut sets);
    Ok(sets)
}

pub(super) fn point_sets(p: &PrimitiveParams, local: &Aabb) -> Result<Vec<Vec<DVec3>>, GeomError> {
    match p.kind {
        Primitive::Stairs => oriented(p, local, stairs),
        Primitive::SpiralStairs => spiral(p, local),
        Primitive::Ramp => oriented(p, local, ramp),
        Primitive::Pillar => {
            super::check_sides(p.sides, 3, "sides")?;
            pillar(p, local)
        }
        Primitive::Doorway => doorway(p, local),
        _ => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive::RiseDirection;
    use crate::test_support::id_counter;
    use crate::{SolidGeom, build_solid, make_primitive};

    fn make(p: &PrimitiveParams) -> Vec<SolidGeom> {
        let mut next = id_counter(0);
        make_primitive(p, &mut next).expect("primitive").iter().map(|s| build_solid(s)).collect()
    }

    fn separated(a: &SolidGeom, b: &SolidGeom) -> bool {
        let apart =
            |x: &SolidGeom, y: &SolidGeom| x.faces.iter().any(|f| y.vertices().all(|v| f.plane.distance(v) >= -1e-6));
        apart(a, b) || apart(b, a)
    }

    fn check(solids: &[SolidGeom], bounds: &Aabb) {
        assert!(!solids.is_empty());
        for s in solids {
            assert!(s.valid);
            assert!(s.volume() > 0.0);
            let c = crate::polygon_center(&s.vertices().collect::<Vec<_>>());
            for f in &s.faces {
                assert!(f.plane.distance(c) < 0.0, "convex");
            }
            assert!(bounds.expand(1e-6).contains_aabb(&s.aabb));
        }
        for (i, a) in solids.iter().enumerate() {
            for b in &solids[i + 1..] {
                assert!(separated(a, b), "overlap");
            }
        }
    }

    fn params(kind: Primitive) -> PrimitiveParams {
        PrimitiveParams {
            kind,
            bounds: Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(64.0, 128.0, 96.0)),
            ..Default::default()
        }
    }

    #[test]
    fn step_count_from_height_or_count() {
        assert_eq!(step_count(96.0, 8, 0.0).unwrap(), 8);
        assert_eq!(step_count(96.0, 8, 16.0).unwrap(), 6);
        assert_eq!(step_count(10.0, 8, 64.0).unwrap(), 1);
        assert!(step_count(96.0, 0, 0.0).is_err());
        assert!(step_count(96.0, MAX_STAIR_STEPS + 1, 0.0).is_err());
        assert!(step_count(96.0, 8, -1.0).is_err());
    }

    #[test]
    fn stairs_rise_and_steps() {
        let p = params(Primitive::Stairs);
        let solids = make(&p);
        assert_eq!(solids.len(), 8);
        check(&solids, &p.bounds);
        let top = solids.iter().map(|s| s.aabb.max.z).fold(f64::MIN, f64::max);
        assert_eq!(top, 96.0);
        let mut tops: Vec<f64> = solids.iter().map(|s| s.aabb.max.z).collect();
        tops.sort_by(f64::total_cmp);
        for (i, t) in tops.iter().enumerate() {
            assert_eq!(*t, 12.0 * (i + 1) as f64);
        }
        let by_height = PrimitiveParams { stair_step_height: 16.0, ..p.clone() };
        assert_eq!(make(&by_height).len(), 6);
        let open = PrimitiveParams { stair_open: true, ..p.clone() };
        let solids = make(&open);
        check(&solids, &p.bounds);
        assert!(solids.iter().skip(2).all(|s| s.aabb.min.z > 0.0));
        let filled: f64 = make(&p).iter().map(|s| s.volume()).sum();
        let hollow: f64 = solids.iter().map(|s| s.volume()).sum();
        assert!(hollow < filled);
    }

    #[test]
    fn stairs_reject_bad_input() {
        let mut next = id_counter(0);
        let bad = PrimitiveParams { stair_steps: 0, ..params(Primitive::Stairs) };
        assert!(make_primitive(&bad, &mut next).is_err());
        let bad = PrimitiveParams { stair_steps: 500, ..params(Primitive::Stairs) };
        assert!(make_primitive(&bad, &mut next).is_err());
    }

    #[test]
    fn spiral_steps_are_disjoint_and_rise() {
        let p = PrimitiveParams {
            bounds: Aabb::new(DVec3::new(-64.0, -64.0, 0.0), DVec3::new(64.0, 64.0, 96.0)),
            stair_steps: 12,
            spiral_angle: 360.0,
            ..params(Primitive::SpiralStairs)
        };
        let solids = make(&p);
        assert_eq!(solids.len(), 12);
        check(&solids, &p.bounds);
        assert_eq!(solids.last().unwrap().aabb.max.z, 96.0);
        let reversed = PrimitiveParams { spiral_angle: -270.0, ..p.clone() };
        check(&make(&reversed), &p.bounds);
        let pie = PrimitiveParams { spiral_inner: 0.0, ..p.clone() };
        check(&make(&pie), &p.bounds);
        let turns = PrimitiveParams { stair_open: true, spiral_angle: 720.0, stair_steps: 24, ..p.clone() };
        check(&make(&turns), &p.bounds);
        let mut next = id_counter(0);
        let solid_turns = PrimitiveParams { stair_open: false, ..turns };
        assert!(make_primitive(&solid_turns, &mut next).is_err());
        let wide = PrimitiveParams { spiral_inner: 70.0, ..p };
        assert!(make_primitive(&wide, &mut next).is_err());
    }

    #[test]
    fn ramp_follows_the_angle() {
        let p = PrimitiveParams { ramp_angle: 45.0, ..params(Primitive::Ramp) };
        let solids = make(&p);
        assert_eq!(solids.len(), 1);
        check(&solids, &p.bounds);
        assert_eq!(solids[0].aabb.max.z, 96.0);
        let plateau = 128.0 - 96.0;
        assert!((solids[0].volume() - 64.0 * (96.0 * 96.0 * 0.5 + 96.0 * plateau)).abs() < 1e-6);
        let shallow = PrimitiveParams { ramp_angle: 10.0, round_vertices: false, ..p.clone() };
        let top = make(&shallow)[0].aabb.max.z;
        assert!((top - 128.0 * 10f64.to_radians().tan()).abs() < 1e-9);
        let mut next = id_counter(0);
        for bad in [0.0, 90.0, f64::NAN] {
            let q = PrimitiveParams { ramp_angle: bad, ..p.clone() };
            assert!(make_primitive(&q, &mut next).is_err());
        }
    }

    #[test]
    fn pillar_parts_stack() {
        let p = params(Primitive::Pillar);
        let solids = make(&p);
        assert_eq!(solids.len(), 3);
        check(&solids, &p.bounds);
        let plain = PrimitiveParams { pillar_base: 0.0, pillar_capital: 0.0, ..p.clone() };
        assert_eq!(make(&plain).len(), 1);
        let base_only = PrimitiveParams { pillar_capital: 0.0, ..p.clone() };
        assert_eq!(make(&base_only).len(), 2);
        let mut next = id_counter(0);
        let bad = PrimitiveParams { pillar_base: 60.0, pillar_capital: 40.0, ..p };
        assert!(make_primitive(&bad, &mut next).is_err());
    }

    #[test]
    fn doorway_leaves_the_opening() {
        let p = PrimitiveParams {
            bounds: Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(128.0, 32.0, 128.0)),
            door_width: 48.0,
            door_height: 96.0,
            ..params(Primitive::Doorway)
        };
        let solids = make(&p);
        assert_eq!(solids.len(), 3);
        check(&solids, &p.bounds);
        let wall = 128.0 * 32.0 * 128.0;
        let volume: f64 = solids.iter().map(|s| s.volume()).sum();
        assert!((volume - (wall - 48.0 * 32.0 * 96.0)).abs() < 1e-6);
        let thin = PrimitiveParams { door_thickness: 8.0, ..p.clone() };
        let solids = make(&thin);
        check(&solids, &p.bounds);
        assert!(solids.iter().all(|s| s.aabb.size().y == 8.0 && s.aabb.min.y == 12.0));
        let mut next = id_counter(0);
        let bad = PrimitiveParams { door_width: 127.0, ..p };
        assert!(make_primitive(&bad, &mut next).is_err());
    }

    #[test]
    fn directions_turn_stairs_and_ramps() {
        let tall = |p: &PrimitiveParams, axis: usize| {
            let verts: Vec<DVec3> = make(p).iter().flat_map(|s| s.vertices().collect::<Vec<_>>()).collect();
            let top = verts.iter().map(|v| v.z).fold(f64::MIN, f64::max);
            let high: Vec<f64> = verts.iter().filter(|v| (v.z - top).abs() < 1e-9).map(|v| v[axis]).collect();
            high.iter().sum::<f64>() / high.len() as f64 - (p.bounds.min[axis] + p.bounds.max[axis]) * 0.5
        };
        let b = Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(128.0, 64.0, 64.0));
        for kind in [Primitive::Stairs, Primitive::Ramp] {
            let base = PrimitiveParams { kind, bounds: b, ..Default::default() };
            let at = |direction: RiseDirection| PrimitiveParams { direction, ..base.clone() };
            for direction in RiseDirection::ALL {
                check(&make(&at(direction)), &b);
            }
            assert!(tall(&at(RiseDirection::PlusY), 1) > 0.0);
            assert!(tall(&at(RiseDirection::MinusY), 1) < 0.0);
            assert!(tall(&at(RiseDirection::PlusX), 0) > 0.0);
            assert!(tall(&at(RiseDirection::MinusX), 0) < 0.0);
            assert!(tall(&at(RiseDirection::Longest), 0) > 0.0);
        }
        let deep = Aabb::new(DVec3::ZERO, DVec3::new(64.0, 128.0, 64.0));
        let p = PrimitiveParams {
            kind: Primitive::Stairs,
            bounds: deep,
            direction: RiseDirection::Longest,
            ..Default::default()
        };
        assert!(tall(&p, 1) > 0.0);
        assert_eq!(RiseDirection::from_id("-X"), Some(RiseDirection::MinusX));
        assert_eq!(RiseDirection::from_id("up"), None);
    }

    #[test]
    fn spiral_starts_toward_the_direction() {
        let bounds = Aabb::new(DVec3::new(-64.0, -64.0, 0.0), DVec3::new(64.0, 64.0, 96.0));
        let first = |direction: RiseDirection| {
            let p = PrimitiveParams {
                bounds,
                direction,
                spiral_angle: 90.0,
                spiral_inner: 0.0,
                ..params(Primitive::SpiralStairs)
            };
            let solids = make(&p);
            check(&solids, &bounds);
            let c = solids[0].aabb.center();
            (c.x, c.y)
        };
        let (_, y) = first(RiseDirection::PlusY);
        assert!(y > 0.0);
        let (x, _) = first(RiseDirection::PlusX);
        assert!(x > 0.0);
        let (_, y) = first(RiseDirection::MinusY);
        assert!(y < 0.0);
        let (x, _) = first(RiseDirection::MinusX);
        assert!(x < 0.0);
    }

    #[test]
    fn doorway_with_an_arch() {
        let p = PrimitiveParams {
            bounds: Aabb::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(128.0, 32.0, 128.0)),
            door_width: 64.0,
            door_height: 96.0,
            door_arch: true,
            door_arch_segments: 8,
            ..params(Primitive::Doorway)
        };
        let solids = make(&p);
        assert_eq!(solids.len(), 2 + 8);
        check(&solids, &p.bounds);
        let wall = 128.0 * 32.0 * 128.0;
        let volume: f64 = solids.iter().map(|s| s.volume()).sum();
        let square = 64.0 * 32.0 * 96.0;
        assert!(volume > wall - square && volume < wall - square * 0.7);
        let segmental = PrimitiveParams { door_arch_radius: 60.0, door_height: 70.0, ..p.clone() };
        check(&make(&segmental), &p.bounds);
        let mut next = id_counter(0);
        for bad in [
            PrimitiveParams { door_arch_radius: 20.0, ..p.clone() },
            PrimitiveParams { door_height: 20.0, ..p.clone() },
            PrimitiveParams { door_arch_segments: 1, ..p.clone() },
            PrimitiveParams { door_arch_segments: 65, ..p.clone() },
        ] {
            assert!(make_primitive(&bad, &mut next).is_err());
        }
    }
}
