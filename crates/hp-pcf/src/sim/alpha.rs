use super::{Operator, V3, b, f, lerp, lerp3, random, simd_random};

pub(super) fn tracer_lifetime(
    clock: (f32, f32, f32),
    dt: f32,
    position: V3,
    previous: V3,
    trail: f32,
) -> (V3, f32, bool) {
    let (time, born, life) = clock;
    if life <= time - born {
        let previous_age = (time - dt) - born;
        if life <= previous_age {
            return (position, trail, true);
        }
        let fraction = (life - previous_age) / dt;
        return (lerp3(previous, position, fraction), trail * fraction, false);
    }
    (position, trail, false)
}

fn smooth(progress: f32) -> f32 {
    let square = progress * progress;
    square * 3.0 - (progress * 2.0) * square
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Simple {
    pub out: bool,
    pub span: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct FadeDecay {
    window: [f32; 4],
    inverses: [f32; 2],
    ends: [f32; 2],
}

impl FadeDecay {
    pub(super) fn compile(op: &Operator) -> Self {
        let mut window = [
            f(op, "start_fade_in_time", 0.0),
            f(op, "end_fade_in_time", 0.5),
            f(op, "start_fade_out_time", 0.5),
            f(op, "end_fade_out_time", 1.0),
        ];
        if window[1] < window[0] {
            window[1] = window[0];
        }
        if window[3] < window[2] {
            window[3] = window[2];
        }
        if window[2] < window[0] {
            window.swap(0, 2);
        }
        if window[3] < window[1] {
            window.swap(1, 3);
        }
        let inverses = [window[1] - window[0], window[3] - window[2]]
            .map(|span| if span == 0.0 { 0.0 } else { wide::f32x4::splat(span).recip().to_array()[0] });
        Self { window, inverses, ends: [f(op, "start_alpha", 1.0), f(op, "end_alpha", 0.0)] }
    }

    pub(super) fn sample(&self, clock: (f32, f32, f32), initial: f32) -> (Option<f32>, bool) {
        let (time, born, life) = clock;
        let age = time - born;
        if life <= age {
            return (None, true);
        }
        let normalized = age * wide::f32x4::splat(life).recip().to_array()[0];
        let mut value = None;
        if self.window[0] <= normalized && normalized < self.window[1] {
            let progress = ((normalized - self.window[0]) * self.inverses[0]).clamp(0.0, 1.0);
            let start = initial * self.ends[0];
            value = Some(start + smooth(progress) * (initial - start));
        }
        if self.window[2] <= normalized && normalized < self.window[3] {
            let progress = ((normalized - self.window[2]) * self.inverses[1]).clamp(0.0, 1.0);
            let delta = initial * self.ends[1] - initial;
            value = Some(initial + smooth(progress) * delta);
        }
        (value, false)
    }
}

impl Simple {
    pub(super) fn value(&self, clock: (f32, f32, f32), initial: f32, current: f32) -> Option<f32> {
        let (time, born, life) = clock;
        let inverse = wide::f32x4::splat(life).recip().to_array()[0];
        let age = (time - born) * inverse;
        let age = if self.out { age } else { age.clamp(0.0, 1.0) };
        let threshold = if self.out { 1.0 - self.span } else { self.span };
        let active = if self.out { age > threshold } else { age < threshold };
        if !active {
            return None;
        }
        let inverse = wide::f32x4::splat(self.span).recip().to_array()[0];
        let progress = ((if self.out { age - threshold } else { age }) * inverse).clamp(0.0, 1.0);
        let factor = smooth(progress);
        Some(if self.out { ((1.0 - factor) * initial).min(current) } else { factor * initial })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Random {
    out: bool,
    spans: [f32; 2],
    quarters: i32,
    proportional: bool,
    ease: bool,
    coefficient: f32,
}

impl Random {
    pub(super) fn compile(op: &Operator, out: bool) -> Self {
        let (minimum, maximum, exponent) = if out {
            ("fade out time min", "fade out time max", "fade out time exponent")
        } else {
            ("fade in time min", "fade in time max", "fade in time exponent")
        };
        let mut spans = [f(op, minimum, 0.25), f(op, maximum, 0.25)];
        if out && spans == [0.0; 2] {
            spans = [f32::EPSILON; 2];
        }
        let bias = f(op, "fade bias", 0.5);
        let bias = if bias == 0.0 { 0.5 } else { bias };
        let inverse = wide::f32x4::splat(bias).recip().to_array()[0];
        Self {
            out,
            spans,
            quarters: (f(op, exponent, 1.0) * 4.0) as i32,
            proportional: b(op, "proportional 0/1", true),
            ease: b(op, "ease in and out", true),
            coefficient: ((inverse + inverse) - (inverse * inverse) * bias) - 2.0,
        }
    }

    pub(super) fn value(&self, clock: (f32, f32, f32), seed: u32, offset: u32, initial: f32) -> Option<f32> {
        let (time, born, life) = clock;
        let random = simd_random::quarter_power(random::sample(seed.wrapping_add(offset)), self.quarters);
        let span = lerp(self.spans[0], self.spans[1], random);
        let age = time - born;
        let age = if self.proportional {
            let normalized = age * wide::f32x4::splat(life).recip().to_array()[0];
            if self.out { normalized } else { normalized.clamp(0.0, 1.0) }
        } else {
            age
        };
        let endpoint = if self.proportional { 1.0 } else { life };
        let threshold = if self.out { endpoint - span } else { span };
        let active = if self.out { age > threshold } else { age < threshold };
        if !active {
            return None;
        }
        let denominator = if self.out { endpoint - threshold } else { span };
        let inverse = wide::f32x4::splat(denominator).recip().to_array()[0];
        let progress = ((if self.out { age - threshold } else { age }) * inverse).clamp(0.0, 1.0);
        Some(if !self.out {
            smooth(progress) * initial
        } else if self.ease {
            (initial + smooth(progress) * (0.0 - initial)).max(0.0)
        } else {
            let factor = progress / (self.coefficient * (1.0 - progress) + 1.0);
            (1.0 - factor) * initial
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmx::{Element, Value};

    #[test]
    fn tracer_endpoints_trail_lengths_and_delayed_death_match_native_float_bits() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/alpha_tracer_fade_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 480);
        for (index, case) in cases.iter().enumerate() {
            for lane in 0..4 {
                let vector = |key: &str| std::array::from_fn(|axis| case[key][lane][axis].as_f64().unwrap() as f32);
                let (position, trail, dead) = tracer_lifetime(
                    (
                        case["time"].as_f64().unwrap() as f32,
                        case["births"][lane].as_f64().unwrap() as f32,
                        case["lifetimes"][lane].as_f64().unwrap() as f32,
                    ),
                    case["dt"].as_f64().unwrap() as f32,
                    vector("positions"),
                    vector("previous"),
                    case["trail"][lane].as_f64().unwrap() as f32,
                );
                let expected = vector("tracer_expected_positions");
                for axis in 0..3 {
                    assert_eq!(position[axis].to_bits(), expected[axis].to_bits(), "case{index} lane{lane} axis{axis}");
                }
                assert_eq!(trail.to_bits(), (case["tracer_expected_trail"][lane].as_f64().unwrap() as f32).to_bits());
                assert_eq!(
                    dead,
                    case["tracer_killed"].as_array().unwrap().iter().any(|v| v.as_u64().unwrap() as usize == lane)
                );
            }
        }
    }

    #[test]
    fn fade_and_decay_matches_native_normalization_curves_and_both_death_rules() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/alpha_fade_decay_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 480);
        for (index, case) in cases.iter().enumerate() {
            let keys = ["start_fade_in_time", "end_fade_in_time", "start_fade_out_time", "end_fade_out_time"];
            let mut attrs: Vec<_> = keys
                .iter()
                .enumerate()
                .map(|(i, key)| ((*key).into(), Value::Float(case["window"][i].as_f64().unwrap() as f32)))
                .collect();
            attrs.push(("start_alpha".into(), Value::Float(case["ends"][0].as_f64().unwrap() as f32)));
            attrs.push(("end_alpha".into(), Value::Float(case["ends"][1].as_f64().unwrap() as f32)));
            let operator =
                Operator { function: "Alpha Fade and Decay".into(), params: Element { attrs, ..Default::default() } };
            let fade = FadeDecay::compile(&operator);
            for (i, actual) in fade.window.iter().enumerate() {
                assert_eq!(actual.to_bits(), (case["normalized_window"][i].as_f64().unwrap() as f32).to_bits());
            }
            for lane in 0..4 {
                let clock = (
                    case["time"].as_f64().unwrap() as f32,
                    case["births"][lane].as_f64().unwrap() as f32,
                    case["lifetimes"][lane].as_f64().unwrap() as f32,
                );
                let (value, dead) = fade.sample(clock, case["initial"][lane].as_f64().unwrap() as f32);
                let value = value.unwrap_or(case["current"][lane].as_f64().unwrap() as f32);
                assert_eq!(
                    value.to_bits(),
                    (case["expected"][lane].as_f64().unwrap() as f32).to_bits(),
                    "case{index} lane{lane}"
                );
                let marked =
                    |key: &str| case[key].as_array().unwrap().iter().any(|v| v.as_u64().unwrap() as usize == lane);
                assert_eq!(dead, marked("killed"), "fade death case{index} lane{lane}");
                assert_eq!(
                    crate::sim::cull::lifespan_expired(clock),
                    marked("lifespan_killed"),
                    "lifespan case{index} lane{lane}"
                );
            }
        }
    }

    #[test]
    fn random_fades_match_native_quarter_powers_bias_and_active_random_slots() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/alpha_random_operator_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 480);
        for (index, case) in cases.iter().enumerate() {
            let out = case["out"].as_bool().unwrap();
            let name = if out { "Alpha Fade Out Random" } else { "Alpha Fade In Random" };
            let key = if out { "fade out time" } else { "fade in time" };
            let operator = Operator {
                function: name.into(),
                params: Element {
                    kind: "DmeParticleOperator".into(),
                    name: name.into(),
                    attrs: vec![
                        (format!("{key} min"), Value::Float(case["span"][0].as_f64().unwrap() as f32)),
                        (format!("{key} max"), Value::Float(case["span"][1].as_f64().unwrap() as f32)),
                        (format!("{key} exponent"), Value::Float(case["exponent"].as_f64().unwrap() as f32)),
                        ("proportional 0/1".into(), Value::Bool(case["proportional"].as_bool().unwrap())),
                        ("ease in and out".into(), Value::Bool(case["ease"].as_bool().unwrap())),
                        ("fade bias".into(), Value::Float(case["bias"].as_f64().unwrap() as f32)),
                    ],
                },
            };
            let fade = Random::compile(&operator, out);
            for lane in 0..4 {
                let current = case["current"][lane].as_f64().unwrap() as f32;
                let actual = fade
                    .value(
                        (
                            case["time"].as_f64().unwrap() as f32,
                            case["births"][lane].as_f64().unwrap() as f32,
                            case["lifetimes"][lane].as_f64().unwrap() as f32,
                        ),
                        (case["seed"].as_u64().unwrap() as u32)
                            .wrapping_add(case["ids"][lane].as_u64().unwrap() as u32),
                        case["offset"].as_u64().unwrap() as u32,
                        case["initial"][lane].as_f64().unwrap() as f32,
                    )
                    .unwrap_or(current);
                let expected = case["expected"][lane].as_f64().unwrap() as f32;
                assert_eq!(actual.to_bits(), expected.to_bits(), "case{index} lane{lane}");
            }
        }
    }

    #[test]
    fn simple_fades_match_native_curves_windows_and_initial_alpha() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/alpha_simple_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 600);
        for (index, case) in cases.iter().enumerate() {
            let fade = Simple { out: case["out"].as_bool().unwrap(), span: case["span"].as_f64().unwrap() as f32 };
            for lane in 0..4 {
                let current = case["current"][lane].as_f64().unwrap() as f32;
                let actual = fade
                    .value(
                        (
                            case["time"].as_f64().unwrap() as f32,
                            case["births"][lane].as_f64().unwrap() as f32,
                            case["lifetimes"][lane].as_f64().unwrap() as f32,
                        ),
                        case["initial"][lane].as_f64().unwrap() as f32,
                        current,
                    )
                    .unwrap_or(current);
                let expected = case["expected"][lane].as_f64().unwrap() as f32;
                assert_eq!(actual.to_bits(), expected.to_bits(), "case{index} lane{lane}");
            }
        }
    }
}
