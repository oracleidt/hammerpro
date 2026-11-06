use super::*;

pub(super) struct Positions<'a> {
    pub current: &'a Points,
    pub previous: &'a Points,
    pub time: f32,
    pub dt: f32,
}

impl Positions<'_> {
    pub(super) fn at(&self, cp: i32, requested: f32) -> V3 {
        let current = self.current.get(cp);
        if self.dt == 0.0 {
            return current;
        }
        // Source clamps past samples, but permits extrapolation past this frame.
        let amount = ((self.dt - (self.time - requested)) / self.dt).max(0.0);
        lerp3(self.previous.get(cp), current, amount)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct PointTrace {
    from: i32,
    to: usize,
    direction: V3,
    length: f32,
    offset: f32,
    interval: f32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct PointTraceState {
    value: Option<V3>,
    next: f32,
}

pub(super) fn compile_point_trace(op: &Operator) -> Option<PointTrace> {
    if !op.function.eq_ignore_ascii_case("Set Control Point to Impact Point") {
        return None;
    }
    Some(PointTrace {
        from: i(op, "Control Point to Trace From", 0),
        to: i(op, "Control Point to Set", 1).clamp(0, MAX_CONTROL_POINTS as i32 - 1) as usize,
        direction: v(op, "Trace Direction Override", [0.0; 3]),
        length: f(op, "Max Trace Length", 1024.0).max(0.0),
        offset: f(op, "Offset End Point Amount", 0.0),
        interval: f(op, "Trace Update Rate", 0.1).max(0.0),
    })
}

fn impact_point(op: &PointTrace, points: &Points, world: &dyn Environment) -> V3 {
    let start = points.get(op.from);
    let direction = if dot(op.direction, op.direction) > 0.0 { op.direction } else { points.orientation(op.from)[0] };
    let end = add(start, scale(direction, op.length));
    let point = match world.trace(start, end, 0.0, false) {
        Some(hit) if hit.fraction.is_finite() => lerp3(start, end, hit.fraction.clamp(0.0, 1.0)),
        _ => end,
    };
    sub(point, scale(direction, op.offset))
}

pub(super) fn update_point_trace(
    op: &PointTrace,
    state: &mut PointTraceState,
    points: &mut Points,
    world: &dyn Environment,
    time: f32,
) {
    if state.value.is_none() {
        state.value = Some(impact_point(op, points, world));
        state.next = op.interval;
    }
    if time >= state.next {
        state.value = Some(impact_point(op, points, world));
        state.next = time + op.interval;
    }
    if points.pos.len() <= op.to {
        points.pos.resize(op.to + 1, [0.0; 3]);
    }
    points.pos[op.to] = state.value.unwrap_or([0.0; 3]);
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ChildPoints {
    group: i32,
    count: usize,
    first: usize,
    particle: usize,
}

pub(super) fn compile_child_points(op: &Operator) -> Option<ChildPoints> {
    if !op.function.eq_ignore_ascii_case("Set child control points from particle positions") {
        return None;
    }
    Some(ChildPoints {
        group: i(op, "Group ID to affect", 0),
        count: i(op, "# of control points to set", 1).clamp(0, MAX_CONTROL_POINTS as i32) as usize,
        first: i(op, "First control point to set", 0).clamp(0, MAX_CONTROL_POINTS as i32 - 1) as usize,
        particle: i(op, "first particle to copy", 0).max(0) as usize,
    })
}

pub(super) fn copy_child_points(op: &ChildPoints, particles: &[Particle], points: &mut Points, group: i32) {
    if op.group != group {
        return;
    }
    for (index, particle) in particles.iter().skip(op.particle).take(op.count).enumerate() {
        let cp = op.first + index;
        if cp >= MAX_CONTROL_POINTS {
            break;
        }
        if points.pos.len() <= cp {
            points.pos.resize(cp + 1, [0.0; 3]);
        }
        points.pos[cp] = particle.pos;
    }
}

mod lock;
pub(super) use lock::{Lock, State as LockState, apply as apply_lock, compile as compile_lock};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Warp {
    cp: i32,
    min: V3,
    max: V3,
    duration: f32,
    start: f32,
    reverse: bool,
}

pub(super) fn compile_warp(op: &Operator) -> Option<Warp> {
    if !op.function.eq_ignore_ascii_case("Position Modify Warp Random") {
        return None;
    }
    Some(Warp {
        cp: i(op, "control point number", 0),
        min: v(op, "warp min", [1.0; 3]),
        max: v(op, "warp max", [1.0; 3]),
        duration: f(op, "warp transition time (treats min/max as start/end sizes)", 0.0),
        start: f(op, "warp transition start time", 0.0),
        reverse: b(op, "reverse warp (0/1)", false),
    })
}

pub(super) fn apply_warp(op: &Warp, particle: &mut Particle, points: &Points, time: f32, rng: &mut Rng) {
    let factors = if op.duration > 0.0 {
        let mut t = ((time - op.start) / op.duration).clamp(0.0, 1.0);
        if op.reverse {
            t = 1.0 - t;
        }
        lerp3(op.min, op.max, t)
    } else {
        rng.range3(op.min, op.max)
    };
    let warp = |value| {
        let local = points.axes.map(|axis| dot(axis, value));
        points.local(mul(local, factors))
    };
    particle.pos = add(points.get(op.cp), warp(sub(particle.pos, points.get(op.cp))));
    particle.previous_pos = add(points.get(op.cp), warp(sub(particle.previous_pos, points.get(op.cp))));
    particle.vel = warp(particle.vel);
}

#[cfg(test)]
mod position_time_tests {
    use super::*;

    #[test]
    fn native_control_point_time_sampling_matches_float_bits() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/cp_position_time_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 300);
        for (index, case) in cases.iter().enumerate() {
            let cp = case["cp"].as_i64().unwrap() as i32;
            let vector = |key: &str| std::array::from_fn(|axis| case[key][axis].as_f64().unwrap() as f32);
            let mut current = Points::at([0.0; 3]);
            current.pos.resize(64, [0.0; 3]);
            let mut previous = current.clone();
            current.pos[cp as usize] = vector("new");
            previous.pos[cp as usize] = vector("old");
            let positions = Positions {
                current: &current,
                previous: &previous,
                time: case["time"].as_f64().unwrap() as f32,
                dt: case["dt"].as_f64().unwrap() as f32,
            };
            let actual = positions.at(cp, case["requested"].as_f64().unwrap() as f32);
            let expected = vector("expected");
            for axis in 0..3 {
                assert_eq!(actual[axis].to_bits(), expected[axis].to_bits(), "case{index} axis{axis}");
            }
        }
    }
}
