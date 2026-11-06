#[derive(Clone, Debug)]
pub(super) struct Stream {
    rows: [[f32; 4]; 55],
    read: usize,
    write: usize,
}

impl Stream {
    pub(super) fn new(mut seed: u32) -> Self {
        let rows = std::array::from_fn(|_| {
            std::array::from_fn(|_| {
                let value = f32::from((seed >> 16) as u16) / 65536.0;
                seed = seed.wrapping_add(1).wrapping_mul(0xbb40_e62d);
                value
            })
        });
        Self { rows, read: 23, write: 54 }
    }

    pub(super) fn next(&mut self) -> [f32; 4] {
        let result = std::array::from_fn(|lane| {
            let sum = self.rows[self.read][lane] + self.rows[self.write][lane];
            if sum >= 1.0 { sum - 1.0 } else { sum }
        });
        self.rows[self.write] = result;
        self.read = if self.read == 0 { 54 } else { self.read - 1 };
        self.write = if self.write == 0 { 54 } else { self.write - 1 };
        result
    }

    pub(super) fn power(&mut self, exponent: f32) -> [f32; 4] {
        let quarters = (exponent * 4.0) as i32;
        self.next().map(|value| quarter_power(value, quarters))
    }
}

fn quarter_power(mut value: f32, quarters: i32) -> f32 {
    let powers = quarters.unsigned_abs();
    let mut result = 1.0;
    if powers & 3 != 0 {
        let root = value.sqrt();
        if powers & 1 != 0 {
            result = root.sqrt();
        }
        if powers & 2 != 0 {
            result *= root;
        }
    }
    let mut integer = powers >> 2;
    while integer != 0 {
        if integer & 1 != 0 {
            result *= value;
        }
        integer >>= 1;
        if integer != 0 {
            value *= value;
        }
    }
    if quarters < 0 {
        let value = if result == 0.0 { f32::from_bits(result.to_bits() | f32::EPSILON.to_bits()) } else { result };
        wide::f32x4::splat(value).recip().to_array()[0]
    } else {
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeded_four_lane_random_stream_matches_native_initialization_and_wraparound() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/simd_random_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 15);
        for case in cases {
            let seed = (case["seed"].as_u64().unwrap() as u32).wrapping_add(case["context"].as_u64().unwrap() as u32);
            let mut stream = Stream::new(seed);
            for expected in case["expected"].as_array().unwrap() {
                for (lane, actual) in stream.next().iter().enumerate() {
                    assert_eq!(actual.to_bits(), (expected[lane].as_f64().unwrap() as f32).to_bits());
                }
            }
        }
    }
}
