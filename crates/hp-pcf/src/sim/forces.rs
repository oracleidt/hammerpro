use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Force {
    Random { min: V3, max: V3 },
    Pull { cp: i32, amount: f32, power: f32 },
    Twist { axis: V3, amount: f32, local: bool },
}

pub(super) fn compile(op: &Operator) -> Option<Force> {
    Some(match op.function.to_ascii_lowercase().as_str() {
        "random force" => Force::Random { min: v(op, "min force", [0.0; 3]), max: v(op, "max force", [0.0; 3]) },
        "pull towards control point" => Force::Pull {
            cp: i(op, "control point number", 0),
            amount: f(op, "amount of force", 100.0),
            power: f(op, "falloff power", 2.0),
        },
        "twist around axis" => Force::Twist {
            axis: v(op, "twist axis", [0.0, 0.0, 1.0]),
            amount: f(op, "amount of force", 100.0),
            local: b(op, "object local space axis 0/1", false),
        },
        _ => return None,
    })
}

pub(super) fn acceleration(force: &Force, part: &Particle, points: &Points, rng: &mut Rng) -> V3 {
    match force {
        Force::Random { min, max } => rng.range3(*min, *max),
        Force::Pull { cp, amount, power } => {
            let direction = sub(points.get(*cp), part.pos);
            let distance = length(direction);
            if distance < 1e-6 {
                return [0.0; 3];
            }
            let magnitude = amount / distance.max(1.0).powf(*power);
            scale(direction, magnitude / distance)
        }
        Force::Twist { axis, amount, local } => {
            let axis = if *local { points.local(*axis) } else { *axis };
            let tangent = cross(sub(part.pos, points.get(0)), axis);
            let size = length(tangent);
            if size > 1e-6 { scale(tangent, amount / size) } else { [0.0; 3] }
        }
    }
}
