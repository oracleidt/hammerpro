use super::*;
use crate::environment::RayEnvironment;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Cull {
    environment: RayEnvironment,
    direction: V3,
    normal: V3,
    velocity: bool,
    adjust_life: bool,
    on_miss: bool,
}

pub(super) fn compile(op: &Operator) -> Option<Cull> {
    op.function.eq_ignore_ascii_case("Cull relative to Ray Trace Environment").then(|| Cull {
        environment: RayEnvironment::from_name(op.params.str("ray trace environment name").unwrap_or("PRECIPITATION")),
        direction: v(op, "test direction", [0.0, 0.0, 1.0]),
        normal: v(op, "cull normal", [0.0; 3]),
        velocity: b(op, "use velocity for test direction", false),
        adjust_life: b(op, "velocity test adjust lifespan", false),
        on_miss: b(op, "cull on miss", false),
    })
}

pub(super) fn initialize(op: &Cull, particle: &mut Particle, dt: f32, world: &dyn Environment) {
    let (direction, speed, maximum) = if op.velocity {
        let dt = if dt > 0.0 { dt } else { 0.025 };
        let previous = sub(particle.pos, scale(particle.vel, dt));
        let movement = sub(particle.pos, previous);
        let squared = dot(movement, movement);
        let speed = squared.sqrt() / dt;
        let direction = scale(movement, wide::f32x4::splat(squared).recip_sqrt().to_array()[0]);
        (direction, speed, particle.life * speed)
    } else {
        (op.direction, 0.0, f32::MAX)
    };
    let hit = direction
        .iter()
        .all(|value| value.is_finite())
        .then(|| world.trace_environment(op.environment, particle.pos, direction, maximum))
        .flatten()
        .filter(|hit| {
            hit.distance.is_finite()
                && hit.distance >= 0.0
                && hit.distance <= maximum
                && (op.normal == [0.0; 3] || op.normal == hit.normal)
        });
    let kill = if let Some(hit) = hit {
        if op.velocity && op.adjust_life {
            particle.life = particle.life.min(hit.distance / (speed + f32::EPSILON));
            false
        } else {
            !op.on_miss || (!op.velocity && op.normal != [0.0; 3])
        }
    } else {
        op.on_miss
    };
    if kill {
        particle.life = -1.0;
        particle.dead = true;
    }
}
