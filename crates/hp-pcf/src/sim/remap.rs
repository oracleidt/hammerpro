use super::{Field, Operator, f, i, lerp};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Scalar {
    pub input: Field,
    pub output: Field,
    input_range: [f32; 2],
    output_range: [f32; 2],
}

impl Scalar {
    pub(super) fn compile(op: &Operator) -> Option<Self> {
        let input = Field::from_index(i(op, "input field", 7).clamp(0, 21));
        let output = Field::from_index(i(op, "output field", 3).clamp(0, 21));
        // Sequence attributes are native floats; their public integer preview fields cannot
        // retain intermediate fractional values yet.
        if matches!(input, Field::Other | Field::Sequence | Field::Sequence2)
            || matches!(output, Field::Other | Field::Sequence | Field::Sequence2)
        {
            return None;
        }
        let mut output_range = [f(op, "output minimum", 0.0), f(op, "output maximum", 1.0)];
        if matches!(output, Field::Alpha | Field::Alpha2) {
            output_range = output_range.map(|value| value.clamp(0.0, 1.0));
        }
        Some(Self {
            input,
            output,
            input_range: [f(op, "input minimum", 0.0), f(op, "input maximum", 1.0)],
            output_range,
        })
    }

    pub(super) fn value(&self, input: f32, current: f32, strength: f32) -> f32 {
        let target = if self.input_range[0] == self.input_range[1] {
            self.output_range[usize::from(input - self.input_range[1] >= 0.0)]
        } else {
            let amount = ((input - self.input_range[0]) / (self.input_range[1] - self.input_range[0])).clamp(0.0, 1.0);
            lerp(self.output_range[0], self.output_range[1], amount)
        };
        current + (target - current) * strength
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dmx::{Element, Value};

    #[test]
    fn scalar_remapping_matches_native_endpoint_clamps_steps_and_strength() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/remap_scalar_operator_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 900);
        for (index, case) in cases.iter().enumerate() {
            let operator = Operator {
                function: "Remap Scalar".into(),
                params: Element {
                    attrs: vec![
                        ("input field".into(), Value::Int(4)),
                        ("output field".into(), Value::Int(case["field"].as_i64().unwrap() as i32)),
                        ("input minimum".into(), Value::Float(case["input_range"][0].as_f64().unwrap() as f32)),
                        ("input maximum".into(), Value::Float(case["input_range"][1].as_f64().unwrap() as f32)),
                        ("output minimum".into(), Value::Float(case["output_range"][0].as_f64().unwrap() as f32)),
                        ("output maximum".into(), Value::Float(case["output_range"][1].as_f64().unwrap() as f32)),
                    ],
                    ..Default::default()
                },
            };
            let remap = Scalar::compile(&operator).unwrap();
            let actual = remap.value(
                case["input"].as_f64().unwrap() as f32,
                case["current"].as_f64().unwrap() as f32,
                case["strength"].as_f64().unwrap() as f32,
            );
            let expected = case["expected"].as_f64().unwrap() as f32;
            assert_eq!(actual.to_bits(), expected.to_bits(), "case{index}");
        }
    }
}
