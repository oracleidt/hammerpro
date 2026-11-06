use super::super::{Operator, Particle, Points, V3, b, f, i, lerp, simd_random::Stream};

#[derive(Clone, Debug, PartialEq)]
pub(in crate::sim) struct Lock {
    cp: i32,
    start: [f32; 2],
    end: [f32; 2],
    exponents: [f32; 2],
    distance: f32,
    rotation: bool,
}

pub(in crate::sim) fn compile(op: &Operator) -> Option<Lock> {
    op.function.eq_ignore_ascii_case("Movement Lock to Control Point").then(|| Lock {
        cp: i(op, "control_point_number", 0).clamp(0, 63),
        start: [f(op, "start_fadeout_min", 1.0), f(op, "start_fadeout_max", 1.0)],
        end: [f(op, "end_fadeout_min", 1.0), f(op, "end_fadeout_max", 1.0)],
        exponents: [f(op, "start_fadeout_exponent", 1.0), f(op, "end_fadeout_exponent", 1.0)],
        distance: f(op, "distance fade range", 0.0),
        rotation: b(op, "lock rotation", false),
    })
}

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::sim) struct State {
    position: V3,
    matrix: [[f32; 4]; 3],
}

fn matrix(points: &Points, cp: i32) -> [[f32; 4]; 3] {
    let axes = points.orientation(cp);
    let position = points.get(cp);
    std::array::from_fn(|row| [axes[0][row], axes[1][row], axes[2][row], position[row]])
}

fn inverse(matrix: [[f32; 4]; 3]) -> [[f32; 4]; 3] {
    std::array::from_fn(|row| {
        let position =
            -((matrix[0][row] * matrix[0][3] + matrix[1][row] * matrix[1][3]) + matrix[2][row] * matrix[2][3]);
        [matrix[0][row], matrix[1][row], matrix[2][row], position]
    })
}

fn concat(a: [[f32; 4]; 3], b: [[f32; 4]; 3]) -> [[f32; 4]; 3] {
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            let value = (a[row][0] * b[0][column] + a[row][1] * b[1][column]) + a[row][2] * b[2][column];
            if column == 3 { value + a[row][3] } else { value }
        })
    })
}

fn rotate(point: V3, matrix: [[f32; 4]; 3], weight: f32) -> V3 {
    std::array::from_fn(|axis| {
        let row = matrix[axis];
        let transformed = ((row[0] * point[0] + row[1] * point[1]) + row[2] * point[2]) + row[3];
        (transformed - point[axis]) * weight + point[axis]
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::sim) fn apply(
    op: &Lock,
    state: &mut State,
    particles: &mut [Particle],
    points: &Points,
    time: f32,
    dt: f32,
    strength: f32,
    random: &mut Stream,
) {
    if particles.is_empty() {
        return;
    }
    let current = matrix(points, op.cp);
    let position = points.get(op.cp);
    if dt <= 0.0 {
        // A zero-time preview step establishes the starting frame without integrating particles.
        if state.position == [0.0; 3] {
            *state = State { position, matrix: current };
        }
        return;
    }
    // The native context uses a zero position as its initialization sentinel.
    let initialize = state.position == [0.0; 3];
    if initialize {
        *state = State { position, matrix: current };
    } else if (!op.rotation && state.position == position)
        || (op.rotation
            && current.iter().flatten().zip(state.matrix.iter().flatten()).all(|(a, b)| (a - b).abs() <= 0.00001))
    {
        return;
    }
    let transform = if op.rotation { concat(current, inverse(state.matrix)) } else { [[0.0; 4]; 3] };
    let movement: V3 = std::array::from_fn(|axis| (position[axis] - state.position[axis]) * strength);
    let fast = op.start[0] >= 1.0;
    let reciprocal = wide::f32x4::splat(0.2).recip().to_array()[0];
    let coefficient = ((reciprocal + reciprocal) - (reciprocal * reciprocal) * 0.2) - 2.0;
    for batch in particles.chunks_mut(4) {
        let mut weights = [1.0; 4];
        if !fast {
            let starts = random.power(op.exponents[0]);
            let ends = random.power(op.exponents[1]);
            for (lane, particle) in batch.iter().enumerate() {
                let age = time - particle.born;
                let normal = (age * wide::f32x4::splat(particle.life).recip().to_array()[0]).clamp(0.0, 1.0);
                let start = lerp(op.start[0], op.start[1], starts[lane]);
                let end = lerp(op.end[0], op.end[1], ends[lane]);
                weights[lane] = 1.0 - ((normal - start) / (end - start)).clamp(0.0, 1.0);
            }
            if !weights[..batch.len()].iter().any(|weight| *weight > 0.0) {
                continue;
            }
        }
        for (lane, particle) in batch.iter_mut().enumerate() {
            let fraction = (time - particle.born).min(dt) * (1.0 / dt);
            let fade = weights[lane];
            let mut delta = movement.map(|value| (value * fraction) * fade);
            let mut rotation_weight = if fast { strength } else { fade };
            if op.distance != 0.0 {
                let relative: V3 = std::array::from_fn(|axis| (particle.pos[axis] + delta[axis]) - position[axis]);
                let distance =
                    ((relative[1] * relative[1] + relative[0] * relative[0]) + relative[2] * relative[2]).sqrt();
                let normal = (distance * (1.0 / op.distance)).min(1.0);
                let biased = normal / ((1.0 - normal) * coefficient + 1.0);
                delta = delta.map(|value| value * (1.0 - biased));
                // Source uses this distinct blend for rotation, rather than scaling the translation blend.
                rotation_weight = 1.0 - biased * rotation_weight;
            }
            if op.rotation {
                let weight = rotation_weight * fraction;
                particle.pos = rotate(particle.pos, transform, weight);
                particle.previous_pos = rotate(particle.previous_pos, transform, weight);
                if particle.previous_dt <= 0.0 {
                    let mut rotation = transform;
                    for row in &mut rotation {
                        row[3] = 0.0;
                    }
                    particle.vel = rotate(particle.vel, rotation, weight);
                }
            } else {
                particle.pos = std::array::from_fn(|axis| particle.pos[axis] + delta[axis]);
                particle.previous_pos = std::array::from_fn(|axis| particle.previous_pos[axis] + delta[axis]);
            }
            if particle.previous_dt > 0.0 {
                particle.vel = std::array::from_fn(|axis| {
                    (particle.pos[axis] - particle.previous_pos[axis]) * (1.0 / particle.previous_dt)
                });
            }
        }
    }
    *state = State { position, matrix: current };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Plan, Sim, SystemDef};
    use std::sync::Arc;

    #[test]
    fn control_point_translation_rotation_birth_fractions_and_fades_match_native_batches() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../../tests/data/lock_cp_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 360);
        for (index, case) in cases.iter().enumerate() {
            let number = |name: &str| case[name].as_f64().unwrap() as f32;
            let vector = |value: &serde_json::Value| std::array::from_fn(|axis| value[axis].as_f64().unwrap() as f32);
            let matrix = |name: &str| {
                std::array::from_fn(|row| {
                    std::array::from_fn(|column| case[name][row][column].as_f64().unwrap() as f32)
                })
            };
            let old = matrix("old_matrix");
            let new: [[f32; 4]; 3] = matrix("new_matrix");
            let mut points = Points::at(vector(&case["new"]));
            points.axes = std::array::from_fn(|column| std::array::from_fn(|row| new[row][column]));
            let op = Lock {
                cp: 0,
                start: std::array::from_fn(|axis| case["start"][axis].as_f64().unwrap() as f32),
                end: std::array::from_fn(|axis| case["end"][axis].as_f64().unwrap() as f32),
                exponents: std::array::from_fn(|axis| case["exponents"][axis].as_f64().unwrap() as f32),
                distance: number("distance"),
                rotation: case["rotation"].as_bool().unwrap(),
            };
            if op.rotation {
                let transform = concat(new, inverse(old));
                for (axis, actual) in transform.iter().flatten().enumerate() {
                    assert_eq!(
                        actual.to_bits(),
                        (case["transform"][axis].as_f64().unwrap() as f32).to_bits(),
                        "matrix case{index} element{axis}"
                    );
                }
            }
            let seed = case["seed"].as_u64().unwrap() as u32;
            let mut sim =
                Sim::new(Arc::new(Plan::new(&SystemDef { initial_particles: 4, ..Default::default() })), seed);
            sim.step(0.0, [0.0; 3], 4);
            for (lane, particle) in sim.particles.iter_mut().enumerate() {
                particle.pos = vector(&case["positions"][lane]);
                particle.previous_pos = vector(&case["previous"][lane]);
                particle.life = case["lifetimes"][lane].as_f64().unwrap() as f32;
                particle.born = case["births"][lane].as_f64().unwrap() as f32;
            }
            let mut state = State { position: vector(&case["old"]), matrix: old };
            let mut random = Stream::new(seed);
            for _ in 0..case["skip"].as_u64().unwrap() {
                random.next();
            }
            apply(
                &op,
                &mut state,
                &mut sim.particles,
                &points,
                number("time"),
                number("dt"),
                number("strength"),
                &mut random,
            );
            for (lane, particle) in sim.particles.iter().enumerate() {
                for (key, actual) in [("expected", particle.pos), ("expected_previous", particle.previous_pos)] {
                    for (axis, value) in actual.iter().enumerate() {
                        assert_eq!(
                            value.to_bits(),
                            (case[key][lane][axis].as_f64().unwrap() as f32).to_bits(),
                            "case{index} lane{lane} {key} axis{axis}"
                        );
                    }
                }
            }
        }
    }
}
