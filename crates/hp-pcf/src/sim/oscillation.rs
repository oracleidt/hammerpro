use super::{Operator, b, f, lerp, random};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Scalar {
    rate: [f32; 2],
    frequency: [f32; 2],
    start: [f32; 2],
    end: [f32; 2],
    proportional: bool,
    window_proportional: bool,
    multiplier: f32,
    phase: f32,
}

impl Scalar {
    pub(super) fn compile(op: &Operator) -> Self {
        Self {
            rate: [f(op, "oscillation rate min", 0.0), f(op, "oscillation rate max", 0.0)],
            frequency: [f(op, "oscillation frequency min", 1.0), f(op, "oscillation frequency max", 1.0)],
            start: [f(op, "start time min", 0.0), f(op, "start time max", 0.0)],
            end: [f(op, "end time min", 1.0), f(op, "end time max", 1.0)],
            proportional: b(op, "proportional 0/1", true),
            window_proportional: b(op, "start/end proportional", true),
            multiplier: f(op, "oscillation multiplier", 2.0),
            phase: f(op, "oscillation start phase", 0.5),
        }
    }

    pub(super) fn delta(&self, frame: (f32, f32, f32), seed: u32, offset: u32, dt: f32) -> Option<f32> {
        let (time, born, life) = frame;
        if life <= 0.0 {
            return None;
        }
        let sample = |key| random::sample(seed.wrapping_add(offset).wrapping_add(key));
        let range = |bounds: [f32; 2], key| lerp(bounds[0], bounds[1], sample(key));
        let age = time - born;
        let normalized = age * wide::f32x4::splat(life).recip().to_array()[0];
        let window = if self.window_proportional { normalized } else { age };
        if window < range(self.start, 11) || window >= range(self.end, 12) {
            return None;
        }
        let frequency = range(self.frequency, 0);
        // Source applies phase before frequency only in the non-proportional path.
        let phase = if self.proportional {
            normalized * frequency * self.multiplier + self.phase
        } else {
            frequency * (time * self.multiplier + self.phase)
        };
        Some(sine(phase) * (range(self.rate, 1) * dt))
    }
}

// Source's SinSIMD is a signed parabolic wave with period two, rather than std::sin.
fn sine(value: f32) -> f32 {
    let mut phase = value.abs();
    let rounded = f32::from_bits((phase + 8388608.0).to_bits() & !1) - 8388608.0;
    phase -= if phase < rounded { rounded - 2.0 } else { rounded };
    let mut sign = value.to_bits() & 0x80000000;
    if phase >= 1.0 {
        phase -= 1.0;
        sign ^= 0x80000000;
    }
    let wave = (4.0 - phase * 4.0) * phase;
    f32::from_bits(wave.to_bits() ^ sign)
}

pub(super) fn before_emitters(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "remap cp speed to cp"
            | "remap particle bbox volume to cp"
            | "remap average scalar value to cp"
            | "set control point positions"
            | "remap distance between two control points to cp"
            | "set control point to player"
            | "set control point to particles' center"
            | "set control point rotation"
            | "set control point to impact point"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_oscillation_matches_native_random_queries_windows_and_simd_wave() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/oscillation_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 240);
        for (index, case) in cases.iter().enumerate() {
            let pair = |key: &str| std::array::from_fn(|i| case[key][i].as_f64().unwrap() as f32);
            let wave = Scalar {
                rate: pair("rates"),
                frequency: pair("frequencies"),
                start: pair("start"),
                end: pair("end"),
                proportional: case["proportional"].as_bool().unwrap(),
                window_proportional: case["window_proportional"].as_bool().unwrap(),
                multiplier: case["multiplier"].as_f64().unwrap() as f32,
                phase: case["phase"].as_f64().unwrap() as f32,
            };
            for lane in 0..4 {
                let at = |key: &str| case[key][lane].as_f64().unwrap() as f32;
                let seed =
                    (case["seed"].as_u64().unwrap() as u32).wrapping_add(case["ids"][lane].as_u64().unwrap() as u32);
                let value = wave.delta(
                    (case["time"].as_f64().unwrap() as f32, at("births"), at("lifetimes")),
                    seed,
                    case["offset"].as_u64().unwrap() as u32,
                    case["dt"].as_f64().unwrap() as f32,
                );
                let actual = value.map_or(at("initial"), |delta| at("initial") + delta);
                assert_eq!(actual.to_bits(), at("expected").to_bits(), "case{index} lane{lane}");
            }
        }
    }
}
