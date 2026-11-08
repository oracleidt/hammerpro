use super::{Emit, Environment, Points};

#[derive(Clone, Debug)]
pub(super) struct State {
    total: f32,
    made: u64,
}

impl State {
    pub(super) fn new(emitter: &Emit) -> Self {
        Self { total: if matches!(emitter, Emit::Noise { .. }) { 1.0 } else { 0.0 }, made: 0 }
    }

    pub(super) fn births(
        &mut self,
        emitter: &Emit,
        frame: (f32, f32),
        points: &Points,
        parent: Option<usize>,
        room: usize,
        world: &dyn Environment,
    ) -> Batch {
        let (time, dt) = frame;
        let highest = points.highest().min(super::MAX_CONTROL_POINTS - 1) as f32;
        let (rate, start, duration) = match emitter {
            Emit::Noise { noise, used, start, duration } => {
                let mut rate = noise.evaluate([0.0; 3], time, world).clamp(0.0, 2147483648.0);
                let factor = highest * used;
                if factor != 0.0 {
                    rate *= factor;
                }
                (rate, *start, *duration)
            }
            Emit::Continuous { rate, used, cp, axis, parent: by_parent, start, duration } => {
                let mut rate = *rate;
                if *cp >= 0 {
                    rate *= points.get(*cp)[*axis].max(0.0);
                }
                let mut factor = highest * used;
                if factor != 0.0 || *by_parent {
                    if *by_parent {
                        factor = parent.map_or(1.0, |count| count as f32) * used;
                    }
                    rate *= factor;
                }
                (rate, *start, *duration)
            }
            Emit::Instant { .. } => return Batch::instant(0.0, 0),
        };
        let previous = time - dt;
        if !rate.is_finite() || rate <= 0.0 || time < start || (duration > 0.0 && previous > start + duration) {
            return Batch::instant(time, 0);
        }
        let (begin, end) =
            if duration > 0.0 { (previous.max(start), time.min(start + duration)) } else { (previous, time) };
        let span = end - begin;
        self.total += span * rate;
        let made = self.total.floor() as u64;
        let requested = made.saturating_sub(self.made);
        self.made = made;
        let count = requested.min(room as u64) as usize;
        let step = if count > 0 { span / count as f32 } else { 0.0 };
        Batch { left: count, cursor: begin + step, step, end }
    }
}

pub(super) struct Batch {
    left: usize,
    cursor: f32,
    step: f32,
    end: f32,
}

impl Batch {
    pub(super) fn instant(time: f32, count: usize) -> Self {
        Self { left: count, cursor: time, step: 0.0, end: time }
    }
}

impl Iterator for Batch {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        let time = self.cursor.min(self.end);
        self.cursor += self.step;
        Some(time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmx::{Element, Value};
    use crate::{Operator, Plan, Sim, SystemDef};
    use std::sync::Arc;

    struct Clock {
        ticks: [u32; 3],
        index: std::cell::Cell<usize>,
    }
    impl Environment for Clock {
        fn time_milliseconds(&self) -> u32 {
            let index = self.index.get();
            self.index.set(index + 1);
            self.ticks[index]
        }
    }

    #[test]
    fn full_game_emission_counts_and_latest_birth_times_match_recorded_frames() {
        let definitions = crate::parse_file(include_bytes!("../../tests/data/emission_live_cases.pcf")).unwrap();
        let frames: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/emission_live_native_samples.json")).unwrap();
        assert_eq!(frames.len(), 149);
        for name in ["n0", "n1", "n2", "n3", "c0", "c1"] {
            let mut definition =
                definitions.iter().find(|def| def.name == format!("hp_spritecard_{name}_count")).unwrap().clone();
            definition.initializers.retain(|op| {
                !matches!(op.function.as_str(), "Remap Particle Count to Scalar" | "Remap Scalar to Vector")
            });
            let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 0);
            let mut points = Points::unparented([0.0; 3]);
            if name == "c1" {
                points.pos[0][1] = 0.5;
            }
            for (index, frame) in frames.iter().enumerate() {
                sim.step_at(frame["dt"].as_f64().unwrap() as f32, &points, &[], 5000);
                let expected = &frame["cases"][name];
                assert_eq!(sim.particles.len(), expected["count"].as_u64().unwrap() as usize, "{name} frame {index}");
                let born = sim.particles.last().map_or(0.0, |particle| particle.born);
                let target = expected["last_birth"].as_f64().unwrap() as f32;
                assert!((born - target).abs() <= 0.00001, "{name} frame {index}: {born} vs {target}");
            }
        }
    }

    #[test]
    fn native_emitter_counts_and_birth_times_match_all_measured_frames() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/emission_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 24);
        for case in cases {
            let number = |key: &str| case[key].as_f64().unwrap() as f32;
            let int = |key: &str| case[key].as_i64().unwrap() as i32;
            let boolean = |key: &str| case[key].as_bool().unwrap();
            let noisy = case["kind"] == "noise";
            let emitter = Operator {
                function: if noisy { "emit noise" } else { "emit_continuously" }.into(),
                params: Element {
                    kind: "DmeParticleOperator".into(),
                    name: String::new(),
                    attrs: vec![
                        ("emission_start_time".into(), Value::Float(number("start"))),
                        ("emission_duration".into(), Value::Float(number("duration"))),
                        ("emission_rate".into(), Value::Float(number("rate"))),
                        ("scale emission to used control points".into(), Value::Float(number("used_cp_scale"))),
                        ("use parent particles for emission scaling".into(), Value::Bool(boolean("parent_scale"))),
                        ("emission count scale control point".into(), Value::Int(int("cp"))),
                        ("emission count scale control point field".into(), Value::Int(int("axis"))),
                        ("emission minimum".into(), Value::Float(number("min"))),
                        ("emission maximum".into(), Value::Float(number("max"))),
                        ("absolute value".into(), Value::Bool(boolean("absolute"))),
                        ("invert absolute value".into(), Value::Bool(boolean("invert"))),
                        ("time noise coordinate scale".into(), Value::Float(number("time_scale"))),
                        ("time coordinate offset".into(), Value::Float(number("time_offset"))),
                        ("world time noise coordinate scale".into(), Value::Float(number("world_scale"))),
                    ],
                },
            };
            let definition =
                SystemDef { max_particles: int("cap") as u32, emitters: vec![emitter], ..Default::default() };
            let mut sim = Sim::new(Arc::new(Plan::new(&definition)), 0);
            sim.parented = !case["parent_count"].is_null();
            let parents = vec![([0.0; 3], [0.0; 3]); case["parent_count"].as_u64().unwrap_or(0) as usize];
            let mut points = Points::at(std::array::from_fn(|axis| case["control"][axis].as_f64().unwrap() as f32));
            points.pos.resize(int("highest") as usize + 1, [0.0; 3]);
            for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
                if step["clear"].as_bool().unwrap() {
                    sim.particles.clear();
                }
                let before = sim.particles.len();
                let world = Clock {
                    ticks: std::array::from_fn(|axis| step["ticks"][axis].as_u64().unwrap() as u32),
                    index: std::cell::Cell::new(0),
                };
                sim.step_in(step["dt"].as_f64().unwrap() as f32, &points, &parents, int("cap") as usize, &world);
                assert_eq!(
                    sim.particles.len(),
                    step["active"].as_u64().unwrap() as usize,
                    "{} frame {index}",
                    case["kind"]
                );
                let actual: Vec<_> = sim.particles[before..].iter().map(|p| p.born.to_bits()).collect();
                let expected: Vec<_> =
                    step["births"].as_array().unwrap().iter().map(|v| (v.as_f64().unwrap() as f32).to_bits()).collect();
                assert_eq!(actual, expected, "{} frame {index}", case["kind"]);
                assert_eq!(sim.time.to_bits(), (step["time"].as_f64().unwrap() as f32).to_bits());
            }
            sim.restart();
            let first = &case["steps"][0];
            let world = Clock {
                ticks: std::array::from_fn(|axis| first["ticks"][axis].as_u64().unwrap() as u32),
                index: std::cell::Cell::new(0),
            };
            sim.step_in(first["dt"].as_f64().unwrap() as f32, &points, &parents, int("cap") as usize, &world);
            assert_eq!(sim.particles.len(), first["active"].as_u64().unwrap() as usize);
        }
    }
}
