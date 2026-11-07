use super::{Operator, b, f, lerp};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Scale {
    window: [f32; 2],
    scales: [f32; 2],
    inverse_window: f32,
    ease: bool,
    bias: f32,
    coefficient: f32,
}

impl Scale {
    pub(super) fn compile(op: &Operator) -> Self {
        let window = [f(op, "start_time", 0.0), f(op, "end_time", 1.0)];
        let span = window[1] - window[0];
        let inverse_window = if span > 0.0 {
            let inverse = wide::f32x4::splat(span).recip().to_array()[0];
            (inverse + inverse) - (inverse * inverse) * span
        } else {
            0.0
        };
        let bias = f(op, "scale_bias", 0.5);
        let bias = if bias == 0.0 { 0.5 } else { bias };
        let inverse = wide::f32x4::splat(bias).recip().to_array()[0];
        Self {
            window,
            scales: [f(op, "radius_start_scale", 1.0), f(op, "radius_end_scale", 1.0)],
            inverse_window,
            ease: b(op, "ease_in_and_out", false),
            bias,
            coefficient: ((inverse + inverse) - (inverse * inverse) * bias) - 2.0,
        }
    }

    pub(super) fn value(&self, clock: (f32, f32, f32), initial: f32) -> Option<f32> {
        let (time, born, life) = clock;
        if self.window[1] <= self.window[0] || life <= 0.0 {
            return None;
        }
        let inverse = wide::f32x4::splat(life).recip().to_array()[0];
        let age = (time - born) * inverse;
        if !(self.window[0] <= age && age < self.window[1]) {
            return None;
        }
        let mut progress = (age - self.window[0]) * self.inverse_window;
        if self.ease {
            let square = progress * progress;
            progress = square * 3.0 - (progress * 2.0) * square;
        } else if self.bias != 0.5 {
            progress /= self.coefficient * (1.0 - progress) + 1.0;
        }
        Some(lerp(self.scales[0], self.scales[1], progress) * initial)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmx::{Element, Value};

    #[test]
    fn native_radius_scale_matches_windows_initial_radius_and_rational_bias() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/radius_scale_operator_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 480);
        for (index, case) in cases.iter().enumerate() {
            let operator = Operator {
                function: "Radius Scale".into(),
                params: Element {
                    attrs: vec![
                        ("start_time".into(), Value::Float(case["window"][0].as_f64().unwrap() as f32)),
                        ("end_time".into(), Value::Float(case["window"][1].as_f64().unwrap() as f32)),
                        ("radius_start_scale".into(), Value::Float(case["scales"][0].as_f64().unwrap() as f32)),
                        ("radius_end_scale".into(), Value::Float(case["scales"][1].as_f64().unwrap() as f32)),
                        ("scale_bias".into(), Value::Float(case["bias"].as_f64().unwrap() as f32)),
                        ("ease_in_and_out".into(), Value::Bool(case["ease"].as_bool().unwrap())),
                    ],
                    ..Default::default()
                },
            };
            let scale = Scale::compile(&operator);
            for lane in 0..4 {
                let current = case["current"][lane].as_f64().unwrap() as f32;
                let actual = scale
                    .value(
                        (
                            case["time"].as_f64().unwrap() as f32,
                            case["births"][lane].as_f64().unwrap() as f32,
                            case["lifetimes"][lane].as_f64().unwrap() as f32,
                        ),
                        case["initial"][lane].as_f64().unwrap() as f32,
                    )
                    .unwrap_or(current);
                let expected = case["expected"][lane].as_f64().unwrap() as f32;
                assert_eq!(actual.to_bits(), expected.to_bits(), "case{index} lane{lane}");
            }
        }
    }
}
