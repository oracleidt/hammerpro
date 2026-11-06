use super::*;
use crate::environment::Environment;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum SpatialOp {
    Collision { bounce: f32, slide: f32, radius_scale: f32, brush_only: bool },
    Plane { cp: i32, point: V3, normal: V3, global_point: bool, global_normal: bool },
    CullPlane { cp: i32, offset: f32, normal: V3 },
    CullSphere { cp: i32, offset: V3, distance: f32, inside: bool },
}

pub(super) fn compile(op: &Operator) -> Option<SpatialOp> {
    Some(match op.function.to_ascii_lowercase().as_str() {
        "collision via traces" => {
            if i(op, "collision mode", 0) != 0 {
                return None;
            }
            SpatialOp::Collision {
                bounce: f(op, "amount of bounce", 0.0).max(0.0),
                slide: f(op, "amount of slide", 0.0).max(0.0),
                radius_scale: f(op, "radius scale", 1.0).max(0.0),
                brush_only: b(op, "brush only", false),
            }
        }
        "prevent passing through a plane" => SpatialOp::Plane {
            cp: i(op, "control point number", 0),
            point: v(op, "plane point", [0.0; 3]),
            normal: v(op, "plane normal", [0.0, 0.0, 1.0]),
            global_point: b(op, "global origin", false),
            global_normal: b(op, "global normal", false),
        },
        "cull when crossing plane" => SpatialOp::CullPlane {
            cp: i(op, "Control Point for point on plane", 0),
            offset: f(op, "Cull plane offset", 0.0),
            normal: v(op, "Plane Normal", [0.0, 0.0, 1.0]),
        },
        "cull when crossing sphere" => SpatialOp::CullSphere {
            cp: i(op, "Control Point", 0),
            offset: v(op, "Control Point offset", [0.0; 3]),
            distance: f(op, "Cull Distance", 100.0).max(0.0),
            inside: b(op, "Cull inside instead of outside", false),
        },
        _ => return None,
    })
}

pub(super) fn apply(op: &SpatialOp, part: &mut Particle, old: V3, points: &Points, dt: f32, world: &dyn Environment) {
    match op {
        SpatialOp::Collision { bounce, slide, radius_scale, brush_only } => {
            let mut start = old;
            let mut end = part.pos;
            let mut remaining = dt;
            for _ in 0..8 {
                let Some(hit) = world.trace(start, end, part.radius * radius_scale, *brush_only) else {
                    part.pos = end;
                    return;
                };
                let len = length(hit.normal);
                if !hit.fraction.is_finite() || !len.is_finite() || len < 1e-6 {
                    part.pos = start;
                    part.vel = [0.0; 3];
                    return;
                }
                let normal = scale(hit.normal, 1.0 / len);
                let fraction = hit.fraction.clamp(0.0, 1.0);
                part.pos = lerp3(start, end, fraction);
                let speed = dot(part.vel, normal);
                if speed < 0.0 {
                    let tangent = sub(part.vel, scale(normal, speed));
                    let movement = sub(end, start);
                    let reflected = sub(movement, scale(normal, 2.0 * dot(movement, normal)));
                    let magnitude_squared = dot(movement, movement);
                    let impulse = if dt > 0.0 && magnitude_squared > 1e-12 {
                        wide::f32x4::splat(magnitude_squared).recip_sqrt().to_array()[0] * bounce / dt
                    } else {
                        0.0
                    };
                    part.vel = add(scale(tangent, *slide), scale(reflected, impulse));
                }
                remaining *= 1.0 - fraction;
                if hit.start_solid || remaining < 1e-6 || length(part.vel) < 1e-6 {
                    return;
                }
                start = part.pos;
                end = add(start, scale(part.vel, remaining));
            }
        }
        SpatialOp::Plane { cp, point, normal, global_point, global_normal } => {
            let center = if *global_point { *point } else { add(points.get(*cp), points.local(*point)) };
            let normal = if *global_normal { *normal } else { points.local(*normal) };
            let len = length(normal);
            if len < 1e-6 {
                return;
            }
            let normal = scale(normal, 1.0 / len);
            let distance = dot(sub(part.pos, center), normal);
            if distance < 0.0 && dot(sub(old, center), normal) >= 0.0 {
                part.pos = sub(part.pos, scale(normal, distance));
                let speed = dot(part.vel, normal).min(0.0);
                part.vel = sub(part.vel, scale(normal, speed));
            }
        }
        SpatialOp::CullPlane { cp, offset, normal } => {
            let len = length(*normal);
            if len > 1e-6 && dot(sub(part.pos, points.get(*cp)), scale(*normal, 1.0 / len)) < *offset {
                part.dead = true;
            }
        }
        SpatialOp::CullSphere { cp, offset, distance, inside } => {
            let within = length(sub(part.pos, add(points.get(*cp), *offset))) < *distance;
            if within == *inside {
                part.dead = true;
            }
        }
    }
}
