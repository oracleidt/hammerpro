use super::{Operator, f, i, lerp, random};

#[derive(Clone, Debug, PartialEq)]
struct Range {
    bounds: [f32; 2],
    seed: u32,
}

impl Range {
    fn sample(&self, seed: u32) -> f32 {
        lerp(self.bounds[0], self.bounds[1], random::sample(seed.wrapping_add(self.seed)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Timing {
    fade: [f32; 4],
    period: f32,
    offset: Range,
    scale: Range,
    strength: Range,
    endcap: i32,
}

impl Timing {
    pub(super) fn compile(op: &Operator) -> Self {
        Self {
            fade: [
                f(op, "operator start fadein", 0.0),
                f(op, "operator end fadein", 0.0),
                f(op, "operator start fadeout", 0.0),
                f(op, "operator end fadeout", 0.0),
            ],
            period: f(op, "operator fade oscillate", 0.0),
            offset: Range {
                bounds: [f(op, "operator time offset min", 0.0), f(op, "operator time offset max", 0.0)],
                seed: i(op, "operator time offset seed", 0) as u32,
            },
            scale: Range {
                bounds: [f(op, "operator time scale min", 1.0), f(op, "operator time scale max", 1.0)],
                seed: i(op, "operator time scale seed", 0) as u32,
            },
            strength: Range {
                bounds: [
                    f(op, "operator strength random scale min", 1.0),
                    f(op, "operator strength random scale max", 1.0),
                ],
                seed: i(op, "operator strength scale seed", 0) as u32,
            },
            endcap: i(op, "operator end cap state", -1),
        }
    }

    fn strength(&self, time: f32, seed: u32, ending: bool) -> Option<f32> {
        if self.endcap != -1 && ending != (self.endcap == 1) {
            return None;
        }
        let [start_in, end_in, start_out, end_out] = self.fade;
        let mut clock = time;
        if self.offset.seed != 0 {
            clock = (clock + self.offset.sample(seed)).max(0.0);
        }
        if self.scale.seed != 0 && clock > start_in {
            let inverse = (1.0 / f64::from(self.scale.sample(seed)).max(0.0001)) as f32;
            clock = inverse * (clock - start_in) + start_in;
        }
        if self.period > 0.0 {
            clock = (f64::from(time / self.period) % 1.0) as f32;
        }
        let mut strength = 0.0;
        if clock >= start_in && !(end_out > 0.0 && clock > end_out) {
            let end_in = end_in.max(start_in);
            let start_out = start_out.max(end_in);
            let end_out = end_out.max(start_out);
            strength = 1.0;
            if clock < end_in && end_in > start_in {
                strength = ((clock - start_in) / (end_in - start_in)).min(1.0);
            }
            if clock > start_out && end_out > start_out {
                strength = strength.min((clock - end_out) / (start_out - end_out));
            }
        }
        if self.strength.seed != 0 {
            let scaled = self.strength.sample(seed) * strength;
            let scaled = if scaled < 0.0 { 0.0 } else { scaled };
            strength = (f64::from(strength) * f64::from(scaled)) as f32;
        }
        Some(strength)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Entry {
    timing: Timing,
    group: usize,
}

impl Entry {
    pub(super) fn new(op: &Operator, category: usize, before_emitters: bool) -> Self {
        Self { timing: Timing::compile(op), group: category * 2 + usize::from(before_emitters) }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct State {
    pub(super) strength: f32,
    pub(super) random_offset: u32,
}

impl State {
    pub(super) fn active(self) -> bool {
        self.strength > 0.0
    }
}

pub(super) fn prepare(entries: &[Entry], states: &mut [State], time: f32, seed: u32, ending: bool) {
    let mut offsets = [0u32; 6];
    for (entry, state) in entries.iter().zip(states) {
        state.strength = entry.timing.strength(time, seed, ending).unwrap_or(0.0);
        state.random_offset = offsets[entry.group];
        if state.active() {
            offsets[entry.group] = offsets[entry.group].wrapping_add(17);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timing(case: &serde_json::Value) -> Timing {
        let number = |name: &str| case[name].as_f64().unwrap() as f32;
        let range = |name: &str| Range {
            bounds: [number(&format!("{name}_min")), number(&format!("{name}_max"))],
            seed: case[format!("{name}_seed")].as_u64().unwrap() as u32,
        };
        Timing {
            fade: [number("start_in"), number("end_in"), number("start_out"), number("end_out")],
            period: number("period"),
            offset: range("offset"),
            scale: range("scale"),
            strength: range("strength"),
            endcap: case["endcap"].as_i64().unwrap() as i32,
        }
    }

    #[test]
    fn random_slots_match_native_operator_loops_when_gates_and_pre_emitters_are_mixed() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/common_schedule_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 180);
        for (index, case) in cases.iter().enumerate() {
            let entries: Vec<_> = case["operators"]
                .as_array()
                .unwrap()
                .iter()
                .map(|op| Entry {
                    timing: timing(op),
                    group: 2 + usize::from(op["before_emitters"].as_bool().unwrap()),
                })
                .collect();
            let mut states = vec![State::default(); entries.len()];
            prepare(
                &entries,
                &mut states,
                case["time"].as_f64().unwrap() as f32,
                case["seed"].as_u64().unwrap() as u32,
                case["ending"].as_bool().unwrap(),
            );
            for (operator, state) in states.iter().enumerate() {
                let expected = &case["expected"][operator];
                assert_eq!(state.active(), !expected.is_null(), "case{index} op{operator}");
                if !expected.is_null() {
                    assert_eq!(
                        state.random_offset,
                        expected["offset"].as_u64().unwrap() as u32,
                        "case{index} op{operator}"
                    );
                    assert_eq!(
                        state.strength.to_bits(),
                        (expected["strength"].as_f64().unwrap() as f32).to_bits(),
                        "case{index} op{operator}"
                    );
                }
            }
        }
    }

    #[test]
    fn operator_strength_matches_native_timing_random_scales_and_endcaps() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/common_operator_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 600);
        for (index, case) in cases.iter().enumerate() {
            let actual = timing(case).strength(
                case["time"].as_f64().unwrap() as f32,
                case["seed"].as_u64().unwrap() as u32,
                case["ending"].as_bool().unwrap(),
            );
            let expected = case["expected"].as_f64().map(|value| value as f32);
            assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits), "case{index}");
            assert_eq!(actual.is_some_and(|value| value > 0.0), case["run"].as_bool().unwrap(), "case{index}");
        }
    }
}
