use super::*;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Lock {
    cp: i32,
    start: [f32; 2],
    end: [f32; 2],
    exponents: [f32; 2],
    distance: f32,
}

pub(super) fn compile_lock(op: &Operator) -> Option<Lock> {
    if !op.function.eq_ignore_ascii_case("Movement Lock to Control Point") {
        return None;
    }
    if f(op, "distance fade range", 0.0) != 0.0 {
        return None;
    }
    Some(Lock {
        cp: i(op, "control_point_number", 0),
        start: [f(op, "start_fadeout_min", 1.0), f(op, "start_fadeout_max", 1.0)],
        end: [f(op, "end_fadeout_min", 1.0), f(op, "end_fadeout_max", 1.0)],
        exponents: [f(op, "start_fadeout_exponent", 1.0), f(op, "end_fadeout_exponent", 1.0)],
        distance: f(op, "distance fade range", 0.0),
    })
}

pub(super) fn apply_lock(op: &Lock, particle: &mut Particle, points: &Points, previous: &Points, key: u32) {
    let time = if particle.life > 0.0 { particle.age / particle.life } else { 0.0 };
    let random = |key, exp: f32| hash01(particle.seed, key).powf(exp.max(0.0));
    let start = lerp(op.start[0], op.start[1], random(key, op.exponents[0]));
    let end = lerp(op.end[0], op.end[1], random(key + 1, op.exponents[1]));
    let mut strength = if end > start {
        1.0 - ((time - start) / (end - start)).clamp(0.0, 1.0)
    } else if time <= end {
        1.0
    } else {
        0.0
    };
    if op.distance > 0.0 {
        strength *= (1.0 - length(sub(particle.pos, points.get(op.cp))) / op.distance).clamp(0.0, 1.0);
    }
    particle.pos = add(particle.pos, scale(sub(points.get(op.cp), previous.get(op.cp)), strength));
}

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
    particle.vel = warp(particle.vel);
}
