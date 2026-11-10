use glam::{DVec2, DVec3};
use hp_vmf::{Side, TexAxis};

use crate::texture::{effective_scale, world_axes};
use crate::util::clean_coord;
use crate::{Aabb, GeomError, WELD_EPS, convex_hull};

mod direction;
mod structures;

pub use direction::RiseDirection;
pub use structures::{MAX_DOOR_ARCH_SEGMENTS, MAX_STAIR_ANGLE, MAX_STAIR_STEPS, step_count};

pub const MAX_PRIMITIVE_SIDES: u32 = 128;
pub const MAX_SPIRAL_ARC: f64 = 3600.0;
const MAX_SPIRAL_HEIGHT: f64 = 65536.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Primitive {
    #[default]
    Block,
    Wedge,
    Cylinder,
    Spike,
    Sphere,
    Arch,
    Torus,
    Stairs,
    SpiralStairs,
    Ramp,
    Pillar,
    Doorway,
}

impl Primitive {
    pub const ALL: [Primitive; 12] = [
        Primitive::Block,
        Primitive::Wedge,
        Primitive::Cylinder,
        Primitive::Spike,
        Primitive::Sphere,
        Primitive::Arch,
        Primitive::Torus,
        Primitive::Stairs,
        Primitive::SpiralStairs,
        Primitive::Ramp,
        Primitive::Pillar,
        Primitive::Doorway,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Primitive::Block => "block",
            Primitive::Wedge => "wedge",
            Primitive::Cylinder => "cylinder",
            Primitive::Spike => "spike",
            Primitive::Sphere => "sphere",
            Primitive::Arch => "arch",
            Primitive::Torus => "torus",
            Primitive::Stairs => "stairs",
            Primitive::SpiralStairs => "spiral_stairs",
            Primitive::Ramp => "ramp",
            Primitive::Pillar => "pillar",
            Primitive::Doorway => "doorway",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PrimitiveParams {
    pub kind: Primitive,
    pub bounds: Aabb,
    pub material: String,
    pub sides: u32,
    pub arch_wall_width: f64,
    pub arch_arc_degrees: f64,
    pub arch_start_angle: f64,
    pub arch_add_height: f64,
    pub torus_ring_width: f64,
    pub torus_tube_sides: u32,
    pub torus_rotation_arc: f64,
    pub torus_rotation_start: f64,
    pub torus_rotation_height: f64,
    pub stair_steps: u32,
    pub stair_step_height: f64,
    pub stair_open: bool,
    pub spiral_inner: f64,
    pub spiral_angle: f64,
    pub spiral_start: f64,
    pub ramp_angle: f64,
    pub pillar_base: f64,
    pub pillar_capital: f64,
    pub pillar_inset: f64,
    pub door_width: f64,
    pub door_height: f64,
    pub door_thickness: f64,
    pub door_arch: bool,
    pub door_arch_radius: f64,
    pub door_arch_segments: u32,
    pub direction: RiseDirection,
    pub up_axis: usize,
    pub up_flip: bool,
    pub tex_scale: f64,
    pub lightmap_scale: i32,
    pub round_vertices: bool,
}

impl Default for PrimitiveParams {
    fn default() -> Self {
        Self {
            kind: Primitive::Block,
            bounds: Aabb::new(DVec3::splat(-64.0), DVec3::splat(64.0)),
            material: "DEV/DEV_MEASUREGENERIC01B".to_owned(),
            sides: 8,
            arch_wall_width: 16.0,
            arch_arc_degrees: 360.0,
            arch_start_angle: 0.0,
            arch_add_height: 0.0,
            torus_ring_width: 32.0,
            torus_tube_sides: 8,
            torus_rotation_arc: 360.0,
            torus_rotation_start: 0.0,
            torus_rotation_height: 0.0,
            stair_steps: 8,
            stair_step_height: 0.0,
            stair_open: false,
            spiral_inner: 16.0,
            spiral_angle: 270.0,
            spiral_start: 0.0,
            ramp_angle: 30.0,
            pillar_base: 16.0,
            pillar_capital: 16.0,
            pillar_inset: 8.0,
            door_width: 0.0,
            door_height: 0.0,
            door_thickness: 0.0,
            door_arch: false,
            door_arch_radius: 0.0,
            door_arch_segments: 8,
            direction: RiseDirection::PlusY,
            up_axis: 2,
            up_flip: false,
            tex_scale: 0.25,
            lightmap_scale: 16,
            round_vertices: true,
        }
    }
}

#[inline]
fn ellipse_point(center: DVec2, rx: f64, ry: f64, degrees: f64) -> DVec2 {
    let (s, c) = degrees.to_radians().sin_cos();
    DVec2::new(clean_coord(center.x + s * rx), clean_coord(center.y + c * ry))
}

fn ring(center: DVec2, rx: f64, ry: f64, count: u32, start: f64) -> Vec<DVec2> {
    let step = 360.0 / f64::from(count);
    (0..count).map(|k| ellipse_point(center, rx, ry, start + step * f64::from(k))).collect()
}

fn strictly_convex(poly: &[DVec2]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut sign = 0.0f64;
    for i in 0..n {
        let (Some(&a), Some(&b), Some(&c)) = (poly.get(i), poly.get((i + 1) % n), poly.get((i + 2) % n)) else {
            return false;
        };
        let cr = (b - a).perp_dot(c - b);
        if cr.abs() < 1e-9 {
            return false;
        }
        if sign == 0.0 {
            sign = cr.signum();
        } else if cr.signum() != sign {
            return false;
        }
    }
    true
}

fn rounded(points: &[DVec2]) -> Vec<DVec2> {
    points.iter().map(|p| p.round() + DVec2::ZERO).collect()
}

pub fn hull_solid(
    points: &[DVec3],
    material: &str,
    tex_scale: f64,
    lightmap_scale: i32,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<Side>, GeomError> {
    let hull = convex_hull(points)?;
    let scale = effective_scale(tex_scale);
    let mut sides = Vec::with_capacity(hull.faces.len());
    for face in &hull.faces {
        let pts = hull.face_plane_points(face);
        let (u, v) = world_axes(face.plane.normal);
        sides.push(Side {
            id: next_side_id(),
            plane: pts,
            material: material.to_owned(),
            uaxis: TexAxis { axis: u, shift: 0.0, scale },
            vaxis: TexAxis { axis: v, shift: 0.0, scale },
            lightmap_scale,
            ..Default::default()
        });
    }
    Ok(sides)
}

#[derive(Clone, Copy)]
struct Frame {
    up: usize,
    flip: bool,
}

fn flip_point(flip: bool, p: DVec3) -> DVec3 {
    if flip { DVec3::new(-p.x, p.y, -p.z) } else { p }
}

fn local_to_world(frame: Frame, l: DVec3) -> DVec3 {
    let l = flip_point(frame.flip, l);
    let w = match frame.up {
        0 => DVec3::new(l.z, l.x, l.y),
        1 => DVec3::new(l.x, -l.z, l.y),
        _ => l,
    };
    w + DVec3::ZERO
}

fn world_to_local(frame: Frame, w: DVec3) -> DVec3 {
    let l = match frame.up {
        0 => DVec3::new(w.y, w.z, w.x),
        1 => DVec3::new(w.x, w.z, -w.y),
        _ => w,
    };
    flip_point(frame.flip, l) + DVec3::ZERO
}

fn extrude(poly: &[DVec2], z0: f64, z1: f64) -> Vec<DVec3> {
    poly.iter().flat_map(|p| [DVec3::new(p.x, p.y, z0), DVec3::new(p.x, p.y, z1)]).collect()
}

fn check_sides(count: u32, min: u32, what: &str) -> Result<(), GeomError> {
    if count < min || count > MAX_PRIMITIVE_SIDES {
        return Err(GeomError::InvalidInput(format!(
            "{what} must be between {min} and {MAX_PRIMITIVE_SIDES}, got {count}"
        )));
    }
    Ok(())
}

pub fn make_primitive(
    params: &PrimitiveParams,
    next_side_id: &mut dyn FnMut() -> u32,
) -> Result<Vec<Vec<Side>>, GeomError> {
    let wb = params.bounds;
    if wb.is_empty() || !wb.min.is_finite() || !wb.max.is_finite() || wb.size().min_element() < WELD_EPS * 10.0 {
        return Err(GeomError::InvalidInput("primitive bounds are empty or flat".to_owned()));
    }
    let up = params.up_axis;
    if up > 2 {
        return Err(GeomError::InvalidInput(format!("up axis must be 0, 1 or 2, got {up}")));
    }
    let frame = Frame { up, flip: params.up_flip };
    let b = Aabb::from_points([world_to_local(frame, wb.min), world_to_local(frame, wb.max)]);
    let size = b.size();
    let (x0, y0, z0) = (b.min.x, b.min.y, b.min.z);
    let (x1, y1, z1) = (b.max.x, b.max.y, b.max.z);
    let center = DVec2::new((x0 + x1) * 0.5, (y0 + y1) * 0.5);
    let (rx, ry, rz) = (size.x * 0.5, size.y * 0.5, size.z * 0.5);
    let mut point_sets: Vec<Vec<DVec3>> = Vec::new();
    match params.kind {
        Primitive::Block => point_sets.push(b.corners().to_vec()),
        Primitive::Wedge => {
            let v = DVec3::new;
            point_sets.push(vec![
                v(x0, y0, z0),
                v(x1, y0, z0),
                v(x1, y1, z0),
                v(x0, y1, z0),
                v(x0, y1, z1),
                v(x1, y1, z1),
            ]);
        }
        Primitive::Cylinder | Primitive::Spike => {
            check_sides(params.sides, 3, "sides")?;
            let mut poly = ring(center, rx, ry, params.sides, 0.0);
            if params.round_vertices {
                let r = rounded(&poly);
                if strictly_convex(&r) {
                    poly = r;
                }
            }
            if params.kind == Primitive::Cylinder {
                point_sets.push(extrude(&poly, z0, z1));
            } else {
                let mut pts: Vec<DVec3> = poly.iter().map(|p| DVec3::new(p.x, p.y, z0)).collect();
                pts.push(DVec3::new(center.x, center.y, z1));
                point_sets.push(pts);
            }
        }
        Primitive::Sphere => {
            check_sides(params.sides, 3, "sides")?;
            let around = params.sides;
            let stacks = (around / 2).max(2);
            let cz = (z0 + z1) * 0.5;
            let rings: Vec<Vec<DVec3>> = (0..=stacks)
                .map(|k| {
                    let phi = 180.0 * f64::from(k) / f64::from(stacks);
                    let (sp, cp) = phi.to_radians().sin_cos();
                    let z = clean_coord(cz + rz * cp);
                    if k == 0 || k == stacks {
                        vec![DVec3::new(center.x, center.y, z)]
                    } else {
                        ring(center, rx * sp, ry * sp, around, 0.0)
                            .into_iter()
                            .map(|p| DVec3::new(p.x, p.y, z))
                            .collect()
                    }
                })
                .collect();
            for pair in rings.windows(2) {
                point_sets.push(pair.iter().flatten().copied().collect());
            }
        }
        Primitive::Arch => {
            check_sides(params.sides, 1, "sides")?;
            let (arc, step) = arc_and_step(params)?;
            let w = params.arch_wall_width;
            if !w.is_finite() || w <= WELD_EPS {
                return Err(GeomError::InvalidInput(format!("invalid arch wall width {w}")));
            }
            let add = if params.arch_add_height.is_finite() { params.arch_add_height } else { 0.0 };
            let n = params.sides;
            let start = if params.arch_start_angle.is_finite() { params.arch_start_angle } else { 0.0 };
            let (irx, iry) = (rx - w, ry - w);
            let pie = irx <= WELD_EPS || iry <= WELD_EPS;
            let full = (arc - 360.0).abs() < 1e-9;
            let make = |round: bool| -> Vec<[DVec2; 4]> {
                let pt = |rx: f64, ry: f64, k: u32| {
                    let k = if full && k == n { 0 } else { k };
                    let p = ellipse_point(center, rx, ry, start + step * f64::from(k));
                    if round { p.round() + DVec2::ZERO } else { p }
                };
                (0..n)
                    .map(|k| {
                        let (ia, ib) = if pie { (center, center) } else { (pt(irx, iry, k), pt(irx, iry, k + 1)) };
                        [pt(rx, ry, k), pt(rx, ry, k + 1), ib, ia]
                    })
                    .collect()
            };
            let quad_ok = |q: &[DVec2; 4]| {
                let mut poly: Vec<DVec2> = q.to_vec();
                poly.dedup_by(|a, b| a.distance_squared(*b) < 1e-12);
                strictly_convex(&poly)
            };
            let mut quads = make(false);
            if params.round_vertices {
                let r = make(true);
                if r.iter().all(quad_ok) {
                    quads = r;
                }
            }
            for (k, q) in quads.iter().enumerate() {
                let lift = add * k as f64;
                point_sets.push(extrude(q, z0 + lift, z1 + lift));
            }
        }
        Primitive::Torus => {
            check_sides(params.sides, 3, "sides")?;
            check_sides(params.torus_tube_sides, 3, "torus tube sides")?;
            let rise = params.torus_rotation_height;
            if !rise.is_finite() || rise.abs() > MAX_SPIRAL_HEIGHT {
                return Err(GeomError::InvalidInput(format!("invalid torus rotation height {rise}")));
            }
            let max_arc = if rise == 0.0 { 360.0 } else { MAX_SPIRAL_ARC };
            let step = sweep_step(params.torus_rotation_arc, max_arc, params.sides)?;
            let rise_step = rise / f64::from(params.sides);
            let w = params.torus_ring_width * 0.5;
            let rmin = rx.min(ry);
            if !w.is_finite() || w <= WELD_EPS || w >= rmin - WELD_EPS {
                return Err(GeomError::InvalidInput(format!(
                    "torus ring width must be between 0 and {}, got {}",
                    2.0 * rmin,
                    params.torus_ring_width
                )));
            }
            let wn = w / rmin;
            let m = params.torus_tube_sides;
            let start = if params.torus_rotation_start.is_finite() { params.torus_rotation_start } else { 0.0 };
            let cz = (z0 + z1) * 0.5;
            let tube: Vec<(f64, f64)> = (0..m)
                .map(|j| {
                    let psi = (360.0 * f64::from(j) + 180.0) / f64::from(m);
                    let (s, c) = psi.to_radians().sin_cos();
                    (1.0 - wn + wn * c, cz + rz * s)
                })
                .collect();
            for k in 0..params.sides {
                let mut pts = Vec::with_capacity(2 * m as usize);
                for end in [k, k + 1] {
                    let (s, c) = (start + step * f64::from(end)).to_radians().sin_cos();
                    let lift = rise_step * f64::from(end);
                    for &(r, z) in &tube {
                        pts.push(DVec3::new(
                            clean_coord(center.x + r * rx * s),
                            clean_coord(center.y + r * ry * c),
                            clean_coord(z + lift),
                        ));
                    }
                }
                point_sets.push(pts);
            }
        }
        Primitive::Stairs | Primitive::SpiralStairs | Primitive::Ramp | Primitive::Pillar | Primitive::Doorway => {
            point_sets = structures::point_sets(params, &b)?;
        }
    }
    let mut reach = wb;
    let lift = finite_or_zero(params.arch_add_height).abs() * f64::from(params.sides)
        + finite_or_zero(params.torus_rotation_height).abs();
    reach.min[up] -= lift;
    reach.max[up] += lift;
    let reach = reach.expand(0.5);
    point_sets
        .iter()
        .map(|pts| {
            let world: Vec<DVec3> = pts.iter().map(|p| local_to_world(frame, *p)).collect();
            let sides = hull_solid(&world, &params.material, params.tex_scale, params.lightmap_scale, next_side_id)?;
            let geom = crate::build_solid(&sides);
            if !geom.valid
                || geom.faces.len() != sides.len()
                || geom.volume() <= 0.0
                || !reach.contains_aabb(&geom.aabb)
            {
                return Err(GeomError::InvalidInput(
                    "the shape is too small or too thin for its box: use fewer sides or steps, or a larger box"
                        .to_owned(),
                ));
            }
            Ok(sides)
        })
        .collect()
}

fn finite_or_zero(v: f64) -> f64 {
    if v.is_finite() { v } else { 0.0 }
}

fn arc_and_step(params: &PrimitiveParams) -> Result<(f64, f64), GeomError> {
    let arc = params.arch_arc_degrees;
    let step = sweep_step(arc, 360.0, params.sides)?;
    Ok((arc.min(360.0), step))
}

fn sweep_step(arc: f64, max_arc: f64, segments: u32) -> Result<f64, GeomError> {
    if !arc.is_finite() || arc <= 0.0 || arc > max_arc + 1e-9 {
        return Err(GeomError::InvalidInput(format!("arc must be in (0, {max_arc}], got {arc}")));
    }
    let step = arc.min(max_arc) / f64::from(segments.max(1));
    if step >= 180.0 - 1e-9 {
        return Err(GeomError::InvalidInput("each segment must cover less than 180 degrees".to_owned()));
    }
    Ok(step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::*;
    use crate::{Plane, SolidGeom, build_solid};

    fn check_solid(sides: &[Side], bounds: &Aabb) {
        let g = build_solid(sides);
        assert!(g.valid);
        assert!(g.faces.len() >= 4 && g.faces.len() == sides.len());
        let c = crate::polygon_center(&g.vertices().collect::<Vec<_>>());
        for f in &g.faces {
            let n = crate::polygon_normal(&f.vertices).expect("normal");
            assert!((n - f.plane.normal).length() < 1e-6);
            assert!(f.plane.distance(c) < 0.0);
        }
        assert!(bounds.expand(1e-6).contains_aabb(&g.aabb), "{:?} not in {:?}", g.aabb, bounds);
        let (verts, _) = crate::unique_vertices(&g);
        assert_eq!(verts.len() + g.faces.len(), g.edges().len() + 2);
        for s in sides {
            let n = Plane::from_side(s).expect("plane").normal;
            assert_eq!(world_axes(n), (s.uaxis.axis, s.vaxis.axis));
            assert_eq!(s.uaxis.scale, 0.25);
        }
    }

    fn make(kind: Primitive, sides: u32) -> (PrimitiveParams, Vec<Vec<Side>>) {
        let params = PrimitiveParams {
            kind,
            sides,
            bounds: Aabb::new(DVec3::new(-64.0, -128.0, 0.0), DVec3::new(64.0, 128.0, 96.0)),
            ..Default::default()
        };
        let mut next = id_counter(0);
        let solids = make_primitive(&params, &mut next).expect("primitive");
        (params, solids)
    }

    #[test]
    fn every_primitive_is_valid() {
        for kind in Primitive::ALL {
            for sides in [3, 4, 8, 12, 32] {
                if kind == Primitive::Arch && sides < 3 {
                    continue;
                }
                let (params, solids) = make(kind, sides);
                assert!(!solids.is_empty(), "{kind:?}");
                let mut ids = Vec::new();
                for s in &solids {
                    check_solid(s, &params.bounds);
                    ids.extend(s.iter().map(|x| x.id));
                }
                let n = ids.len();
                ids.sort_unstable();
                ids.dedup();
                assert_eq!(ids.len(), n, "unique side ids");
            }
        }
    }

    #[test]
    fn block_and_wedge() {
        let (_, solids) = make(Primitive::Block, 4);
        assert_eq!(solids.len(), 1);
        let g = build_solid(&solids[0]);
        assert!((g.volume() - 128.0 * 256.0 * 96.0).abs() < 1e-6);
        let (_, solids) = make(Primitive::Wedge, 4);
        let g = build_solid(&solids[0]);
        assert_eq!(g.faces.len(), 5);
        assert!((g.volume() - 128.0 * 256.0 * 96.0 * 0.5).abs() < 1e-6);
    }

    #[test]
    fn cylinder_integer_vertices_and_volume() {
        let (_, solids) = make(Primitive::Cylinder, 8);
        assert_eq!(solids.len(), 1);
        let g = build_solid(&solids[0]);
        assert_eq!(g.faces.len(), 10);
        for v in g.vertices() {
            assert_eq!(v, v.round());
        }
        let (_, solids) = make(Primitive::Cylinder, 64);
        let g = build_solid(&solids[0]);
        let ellipse = std::f64::consts::PI * 64.0 * 128.0 * 96.0;
        assert!((g.volume() - ellipse).abs() / ellipse < 0.01);
    }

    #[test]
    fn sphere_and_spike() {
        let (_, solids) = make(Primitive::Sphere, 16);
        assert_eq!(solids.len(), 8);
        let vol: f64 = solids.iter().map(|s| build_solid(s).volume()).sum();
        let ellipsoid = 4.0 / 3.0 * std::f64::consts::PI * 64.0 * 128.0 * 48.0;
        assert!(vol < ellipsoid && vol > ellipsoid * 0.85, "{vol} vs {ellipsoid}");
        let (_, solids) = make(Primitive::Spike, 4);
        let g = build_solid(&solids[0]);
        assert_eq!(g.faces.len(), 5);
        assert!((g.volume() - 128.0 * 256.0 * 0.5 * 96.0 / 3.0).abs() < 1e-6);
    }

    #[test]
    fn arch_variants() {
        let mut next = id_counter(0);
        let base = PrimitiveParams {
            kind: Primitive::Arch,
            sides: 8,
            bounds: Aabb::new(DVec3::new(-128.0, -128.0, 0.0), DVec3::new(128.0, 128.0, 16.0)),
            arch_wall_width: 32.0,
            arch_arc_degrees: 180.0,
            arch_start_angle: 90.0,
            ..Default::default()
        };
        let solids = make_primitive(&base, &mut next).expect("arch");
        assert_eq!(solids.len(), 8);
        for s in &solids {
            check_solid(s, &base.bounds);
        }
        let stairs = PrimitiveParams { arch_add_height: 16.0, arch_arc_degrees: 360.0, ..base.clone() };
        let solids = make_primitive(&stairs, &mut next).expect("stairs");
        let top = build_solid(solids.last().expect("last"));
        assert_eq!(top.aabb.max.z, 16.0 + 7.0 * 16.0);
        let pie = PrimitiveParams { arch_wall_width: 500.0, ..base.clone() };
        let solids = make_primitive(&pie, &mut next).expect("pie");
        for s in &solids {
            check_solid(s, &pie.bounds);
        }
        let bad = PrimitiveParams { arch_arc_degrees: 0.0, ..base.clone() };
        assert!(make_primitive(&bad, &mut next).is_err());
        let bad = PrimitiveParams { sides: 1, arch_arc_degrees: 360.0, ..base };
        assert!(make_primitive(&bad, &mut next).is_err());
    }

    #[test]
    fn torus_segments() {
        let mut next = id_counter(0);
        let p = PrimitiveParams {
            kind: Primitive::Torus,
            sides: 12,
            torus_tube_sides: 6,
            torus_ring_width: 48.0,
            bounds: Aabb::new(DVec3::new(-128.0, -128.0, -24.0), DVec3::new(128.0, 128.0, 24.0)),
            ..Default::default()
        };
        let solids = make_primitive(&p, &mut next).expect("torus");
        assert_eq!(solids.len(), 12);
        for s in &solids {
            check_solid(s, &p.bounds);
            assert_eq!(s.len(), 8);
        }
        let bad = PrimitiveParams { torus_ring_width: 400.0, ..p };
        assert!(make_primitive(&bad, &mut next).is_err());
    }

    #[test]
    fn primitives_stand_along_the_up_axis() {
        let bounds = Aabb::new(DVec3::new(-32.0, -64.0, 0.0), DVec3::new(32.0, 64.0, 96.0));
        let mut next = id_counter(0);
        for kind in Primitive::ALL {
            let z = PrimitiveParams { kind, bounds, sides: 8, ..Default::default() };
            let z_solids = make_primitive(&z, &mut next).expect("z");
            let z_volume: f64 = z_solids.iter().map(|s| build_solid(s).volume()).sum();
            for up in [0, 1] {
                let p = PrimitiveParams { up_axis: up, ..z.clone() };
                let solids = make_primitive(&p, &mut next).expect("rotated");
                assert_eq!(solids.len(), z_solids.len(), "{kind:?} {up}");
                for s in &solids {
                    check_solid(s, &bounds);
                }
                let volume: f64 = solids.iter().map(|s| build_solid(s).volume()).sum();
                assert!(volume > 0.0);
                if kind == Primitive::Block {
                    assert!((volume - z_volume).abs() < 1e-6);
                }
            }
        }
        let p = PrimitiveParams { kind: Primitive::Cylinder, bounds, sides: 8, up_axis: 0, ..Default::default() };
        let cyl = make_primitive(&p, &mut next).expect("cylinder").remove(0);
        let caps: Vec<DVec3> =
            cyl.iter().filter_map(Plane::from_side).map(|pl| pl.normal).filter(|n| n.x.abs() > 0.999).collect();
        assert_eq!(caps.len(), 2);
        for v in build_solid(&cyl).vertices() {
            assert_eq!(v, v.round(), "exact permutation keeps integer vertices");
        }
        for (up, apex) in
            [(2, DVec3::new(0.0, 0.0, 96.0)), (0, DVec3::new(32.0, 0.0, 48.0)), (1, DVec3::new(0.0, -64.0, 48.0))]
        {
            let p = PrimitiveParams { kind: Primitive::Spike, bounds, sides: 4, up_axis: up, ..Default::default() };
            let g = build_solid(&make_primitive(&p, &mut next).expect("spike")[0]);
            assert!(g.vertices().any(|v| v == apex), "up {up}: apex {apex}");
        }
        for (up, apex) in [(2, DVec3::new(0.0, 0.0, 0.0)), (0, DVec3::new(-32.0, 0.0, 48.0))] {
            let p = PrimitiveParams {
                kind: Primitive::Spike,
                bounds,
                sides: 4,
                up_axis: up,
                up_flip: true,
                ..Default::default()
            };
            let g = build_solid(&make_primitive(&p, &mut next).expect("spike")[0]);
            assert!(g.vertices().any(|v| v == apex), "flipped up {up}: apex {apex}");
            assert!(bounds.expand(1e-9).contains_aabb(&g.aabb));
        }
        let bad = PrimitiveParams { up_axis: 3, ..Default::default() };
        assert!(make_primitive(&bad, &mut next).is_err());
    }

    #[test]
    fn spiral_torus() {
        let mut next = id_counter(0);
        let p = PrimitiveParams {
            kind: Primitive::Torus,
            sides: 16,
            torus_tube_sides: 6,
            torus_ring_width: 32.0,
            torus_rotation_arc: 720.0,
            torus_rotation_start: 90.0,
            torus_rotation_height: 256.0,
            bounds: Aabb::new(DVec3::new(-128.0, -128.0, 0.0), DVec3::new(128.0, 128.0, 32.0)),
            ..Default::default()
        };
        let solids = make_primitive(&p, &mut next).expect("spiral");
        assert_eq!(solids.len(), 16);
        let geoms: Vec<SolidGeom> = solids.iter().map(|s| build_solid(s)).collect();
        for g in &geoms {
            assert!(g.valid);
        }
        let top = geoms.last().expect("last").aabb.max.z;
        assert!((top - (32.0 + 256.0)).abs() < 1e-6, "{top}");
        for w in geoms.windows(2) {
            let shared = w[0].faces.iter().any(|f| {
                w[1].faces.iter().any(|g| {
                    f.plane.approx_eq(&g.plane.flip(), 1e-9, 1e-6)
                        && f.vertices.iter().all(|v| g.vertices.iter().any(|u| u.distance(*v) < 1e-6))
                })
            });
            assert!(shared, "segments must meet face to face");
        }
        let flat = PrimitiveParams { torus_rotation_height: 0.0, ..p.clone() };
        assert!(make_primitive(&flat, &mut next).is_err());
        let too_long = PrimitiveParams { torus_rotation_arc: MAX_SPIRAL_ARC + 1.0, sides: 64, ..p.clone() };
        assert!(make_primitive(&too_long, &mut next).is_err());
        let nan = PrimitiveParams { torus_rotation_height: f64::NAN, ..p.clone() };
        assert!(make_primitive(&nan, &mut next).is_err());
        let half = PrimitiveParams { torus_rotation_arc: 180.0, torus_rotation_height: 0.0, sides: 8, ..p };
        assert_eq!(make_primitive(&half, &mut next).expect("half").len(), 8);
    }

    #[test]
    fn hull_solid_of_an_extruded_outline() {
        let mut next = id_counter(0);
        let outline = [(0.0, 0.0), (64.0, 0.0), (96.0, 48.0), (32.0, 80.0), (-16.0, 40.0)];
        let pts: Vec<DVec3> =
            outline.iter().flat_map(|&(x, y)| [DVec3::new(x, y, 0.0), DVec3::new(x, y, 32.0)]).collect();
        let s = hull_solid(&pts, "TEST/MAT", 0.5, 32, &mut next).expect("prism");
        assert_eq!(s.len(), 7);
        assert!(s.iter().all(|x| x.material == "TEST/MAT" && x.lightmap_scale == 32 && x.uaxis.scale == 0.5));
        assert!(build_solid(&s).valid);
        let flat: Vec<DVec3> = outline.iter().map(|&(x, y)| DVec3::new(x, y, 0.0)).collect();
        assert!(hull_solid(&flat, "TEST/MAT", 0.25, 16, &mut next).is_err());
    }

    #[test]
    fn bad_bounds_and_sides() {
        let mut next = id_counter(0);
        let flat =
            PrimitiveParams { bounds: Aabb::new(DVec3::ZERO, DVec3::new(10.0, 10.0, 0.0)), ..Default::default() };
        assert!(make_primitive(&flat, &mut next).is_err());
        let empty = PrimitiveParams { bounds: Aabb::EMPTY, ..Default::default() };
        assert!(make_primitive(&empty, &mut next).is_err());
        let many = PrimitiveParams { kind: Primitive::Cylinder, sides: 100_000, ..Default::default() };
        assert!(make_primitive(&many, &mut next).is_err());
        let few = PrimitiveParams { kind: Primitive::Cylinder, sides: 2, ..Default::default() };
        assert!(make_primitive(&few, &mut next).is_err());
        let tiny = PrimitiveParams {
            kind: Primitive::Cylinder,
            sides: 32,
            bounds: Aabb::new(DVec3::ZERO, DVec3::splat(4.0)),
            ..Default::default()
        };
        let s = make_primitive(&tiny, &mut next).expect("tiny");
        assert_eq!(build_solid(&s[0]).faces.len(), 34);
    }
}
