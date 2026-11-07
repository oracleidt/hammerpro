#[derive(Clone, Debug, PartialEq)]
pub(super) struct Simple {
    pub out: bool,
    pub span: f32,
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
        let square = progress * progress;
        let smooth = square * 3.0 - (progress * 2.0) * square;
        Some(if self.out { ((1.0 - smooth) * initial).min(current) } else { smooth * initial })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
