use super::*;

#[derive(Clone, Debug, PartialEq)]
enum Input {
    Control { cp: i32, axis: usize },
    Distance { cp: i32, los: bool, max_trace: f32, failure: f32 },
    Dot { first: i32, second: i32, velocity: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Remap {
    input: Input,
    pub field: Field,
    min: f32,
    max: f32,
    out_min: f32,
    out_max: f32,
    initial: bool,
    current: bool,
    within: bool,
    lifetime: [f32; 2],
}

pub(super) fn compile(op: &Operator) -> Option<Remap> {
    let name = op.function.to_ascii_lowercase();
    let input = match name.as_str() {
        "remap control point to scalar" => Input::Control {
            cp: i(op, "input control point number", 0),
            axis: i(op, "input field 0-2 X/Y/Z", 0).clamp(0, 2) as usize,
        },
        "remap distance to control point to scalar" => Input::Distance {
            cp: i(op, "control point", 0),
            los: b(op, "ensure line of sight", false),
            max_trace: f(op, "Maximum Trace Length", -1.0),
            failure: f(op, "LOS Failure Scalar", 0.0),
        },
        "remap dot product to scalar" => Input::Dot {
            first: i(op, "first input control point", 0),
            second: i(op, "second input control point", 0),
            velocity: b(op, "use particle velocity for first input", false),
        },
        _ => return None,
    };
    let field = Field::from_index(i(op, "output field", 3));
    if matches!(input, Input::Distance { los: true, .. }) {
        return None;
    }
    if field == Field::Other {
        return None;
    }
    let (min, max, within) = match input {
        Input::Distance { .. } => ("distance minimum", "distance maximum", "only active within specified distance"),
        Input::Dot { .. } => {
            ("input minimum (-1 to 1)", "input maximum (-1 to 1)", "only active within specified input range")
        }
        Input::Control { .. } => ("input minimum", "input maximum", "only active within specified input range"),
    };
    Some(Remap {
        input,
        field,
        min: f(op, min, if matches!(name.as_str(), "remap dot product to scalar") { -1.0 } else { 0.0 }),
        max: f(op, max, 1.0),
        out_min: f(op, "output minimum", 0.0),
        out_max: f(op, "output maximum", 1.0),
        initial: b(op, "output is scalar of initial random range", false),
        current: b(op, "output is scalar of current value", false),
        within: b(op, within, false),
        lifetime: [
            f(op, "emitter lifetime start time (seconds)", -1.0),
            f(op, "emitter lifetime end time (seconds)", -1.0),
        ],
    })
}

pub(super) fn evaluate(
    op: &Remap,
    p: &Particle,
    points: &Points,
    world: &dyn Environment,
    time: f32,
    initialized: bool,
) -> Option<f32> {
    if (op.lifetime[0] >= 0.0 && time < op.lifetime[0]) || (op.lifetime[1] >= 0.0 && time > op.lifetime[1]) {
        return None;
    }
    let input = match op.input {
        Input::Control { cp, axis } => points.get(cp)[axis],
        Input::Distance { cp, los, max_trace, failure } => {
            let direction = sub(points.get(cp), p.pos);
            let distance = length(direction);
            if los && distance > 1e-6 {
                let endpoint = if max_trace > 0.0 && distance > max_trace {
                    add(p.pos, scale(direction, max_trace / distance))
                } else {
                    points.get(cp)
                };
                if world.trace(p.pos, endpoint, 0.0, false).is_some() {
                    return Some(failure);
                }
            }
            distance
        }
        Input::Dot { first, second, velocity } => {
            let first = if velocity {
                let size = length(p.vel);
                if size > 1e-6 { scale(p.vel, 1.0 / size) } else { [0.0; 3] }
            } else {
                points.orientation(first)[0]
            };
            dot(first, points.orientation(second)[0])
        }
    };
    if op.within && (input < op.min.min(op.max) || input > op.min.max(op.max)) {
        return None;
    }
    let t = if op.max != op.min { ((input - op.min) / (op.max - op.min)).clamp(0.0, 1.0) } else { 0.0 };
    let value = lerp(op.out_min, op.out_max, t);
    let current = if initialized {
        match op.field {
            Field::Radius => p.radius,
            Field::Alpha => p.alpha,
            _ => get_field(p, op.field),
        }
    } else {
        get_field(p, op.field)
    };
    Some(
        value
            * if op.initial {
                if initialized { p.initial_values[op.field as usize] } else { current }
            } else if op.current {
                current
            } else {
                1.0
            },
    )
}
