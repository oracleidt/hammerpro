use super::{Operator, b, f, i, lerp, random};

#[derive(Clone, Debug, PartialEq)]
enum Shape {
    Linear,
    SplineRandom { coefficient: f32, ease_out: bool },
    SplineSimple { ease_out: bool },
}

fn bias_coefficient(bias: f32) -> f32 {
    let inverse = wide::f32x4::splat(bias).recip().to_array()[0];
    ((inverse + inverse) - (inverse * inverse) * bias) - 2.0
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Ramp {
    rate: [f32; 2],
    random: bool,
    start: [f32; 2],
    end: [f32; 2],
    proportional: bool,
    windowed: bool,
    bounds: [f32; 2],
    shape: Shape,
}

impl Ramp {
    pub(super) fn compile(op: &Operator) -> Self {
        let name = op.function.to_ascii_lowercase();
        let random = name.ends_with("random");
        let spline = name.contains("spline");
        let start = if random {
            [f(op, "start time min", 0.0), f(op, "start time max", 0.0)]
        } else {
            [f(op, "start time", 0.0); 2]
        };
        let end =
            if random { [f(op, "end time min", 1.0), f(op, "end time max", 1.0)] } else { [f(op, "end time", 1.0); 2] };
        let proportional = !random || b(op, "start/end proportional", true);
        let field = i(op, "ramp field", 3).clamp(0, 21);
        Self {
            random,
            rate: if random {
                [f(op, "ramp rate min", 0.0), f(op, "ramp rate max", 0.0)]
            } else {
                [f(op, "ramp rate", 0.0); 2]
            },
            start,
            end,
            proportional,
            windowed: !(random && !spline && proportional && start == [0.0; 2] && end == [1.0; 2]),
            bounds: match field {
                7 | 16 => [0.0, 1.0],
                3 | 10 => [0.0, f32::MAX],
                _ => [-f32::MAX, f32::MAX],
            },
            shape: if !spline {
                Shape::Linear
            } else if random {
                Shape::SplineRandom {
                    coefficient: bias_coefficient(f(op, "bias", 0.5)),
                    ease_out: b(op, "ease out", false),
                }
            } else {
                Shape::SplineSimple { ease_out: b(op, "ease out", false) }
            },
        }
    }

    pub(super) fn apply(&self, frame: (f32, f32, f32), seed: u32, offset: u32, dt: f32, value: f32) -> Option<f32> {
        let (time, born, life) = frame;
        if life <= 0.0 || life.is_nan() {
            return None;
        }
        let seed = seed.wrapping_add(offset);
        let range = |bounds: [f32; 2], query: u32| {
            if self.random { lerp(bounds[0], bounds[1], random::sample(seed.wrapping_add(query))) } else { bounds[0] }
        };
        let mut clock = time - born;
        if self.proportional {
            clock *= wide::f32x4::splat(life).recip().to_array()[0];
        }
        let start = range(self.start, 11);
        let end = range(self.end, 12);
        if self.windowed && !(start <= clock && clock < end) {
            return None;
        }
        let rate = range(self.rate, 0);
        let delta = match self.shape {
            Shape::Linear => rate * dt,
            Shape::SplineRandom { coefficient, ease_out } => {
                let progress = progress(clock, start, end, ease_out);
                let amount = progress / (coefficient * (1.0 - progress) + 1.0);
                (amount * dt) * rate
            }
            Shape::SplineSimple { ease_out } => {
                let progress = progress(clock, start, end, ease_out);
                let squared = progress * progress;
                let amount = squared * 3.0 - (progress * 2.0) * squared;
                amount * (dt * rate)
            }
        };
        Some((value + delta).clamp(self.bounds[0], self.bounds[1]))
    }
}

fn progress(clock: f32, start: f32, end: f32, ease_out: bool) -> f32 {
    let progress = ((1.0 / ((end - start) + f32::EPSILON)) * (clock - start)).clamp(0.0, 1.0);
    if ease_out { (if progress > 0.5 { 1.0 - progress } else { progress }) * 2.0 } else { progress }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_four_ramps_match_native_windows_random_samples_shapes_and_attribute_limits() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/ramp_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 960);
        for (index, case) in cases.iter().enumerate() {
            let number = |name: &str| case[name].as_f64().unwrap() as f32;
            let range = |name: &str| std::array::from_fn(|axis| case[name][axis].as_f64().unwrap() as f32);
            let shape = match case["kind"].as_str().unwrap() {
                "linear_random" | "linear_simple" => Shape::Linear,
                "spline_random" => Shape::SplineRandom {
                    coefficient: bias_coefficient(number("bias")),
                    ease_out: case["ease_out"].as_bool().unwrap(),
                },
                "spline_simple" => Shape::SplineSimple { ease_out: case["ease_out"].as_bool().unwrap() },
                _ => unreachable!(),
            };
            let ramp = Ramp {
                random: case["kind"].as_str().unwrap().ends_with("random"),
                rate: range("rates"),
                start: range("start"),
                end: range("end"),
                proportional: case["proportional"].as_bool().unwrap(),
                windowed: case["windowed"].as_bool().unwrap(),
                bounds: [number("minimum"), number("maximum")],
                shape,
            };
            for lane in 0..4 {
                let value = case["initial"][lane].as_f64().unwrap() as f32;
                let actual = ramp
                    .apply(
                        (
                            number("time"),
                            case["births"][lane].as_f64().unwrap() as f32,
                            case["lifetimes"][lane].as_f64().unwrap() as f32,
                        ),
                        (case["seed"].as_u64().unwrap() as u32)
                            .wrapping_add(case["ids"][lane].as_u64().unwrap() as u32),
                        case["offset"].as_u64().unwrap() as u32,
                        number("dt") * number("strength"),
                        value,
                    )
                    .unwrap_or(value);
                let expected = case["expected"][lane].as_f64().unwrap() as f32;
                assert_eq!(actual.to_bits(), expected.to_bits(), "case{index} lane{lane}");
            }
        }
    }
}
