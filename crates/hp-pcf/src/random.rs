#[derive(Clone, Debug)]
pub struct UniformRandomStream {
    state: i32,
    shuffle: [i32; 32],
    last: i32,
}

impl UniformRandomStream {
    pub fn new(seed: i32) -> Self {
        Self { state: -seed.saturating_abs().max(1), shuffle: [0; 32], last: 0 }
    }

    fn advance(&mut self) {
        let quotient = self.state / 127773;
        self.state = 16807 * (self.state - quotient * 127773) - 2836 * quotient;
        if self.state < 0 {
            self.state += i32::MAX;
        }
    }

    pub fn next_int(&mut self) -> i32 {
        if self.state <= 0 || self.last == 0 {
            self.state = self.state.saturating_neg().max(1);
            for index in (0..40).rev() {
                self.advance();
                if index < 32 {
                    self.shuffle[index] = self.state;
                }
            }
            self.last = self.shuffle[0];
        }
        self.advance();
        let index = (self.last / 67108864) as usize;
        self.last = self.shuffle[index];
        self.shuffle[index] = self.state;
        self.last
    }

    pub fn float(&mut self, min: f32, max: f32) -> f32 {
        let unit = ((1.0f32 / i32::MAX as f32) * self.next_int() as f32).min(1.0 - 1.2e-7);
        min + unit * (max - min)
    }

    pub fn int(&mut self, min: i32, max: i32) -> i32 {
        let span = i64::from(max) - i64::from(min) + 1;
        if span <= 1 || span > i64::from(i32::MAX) {
            return min;
        }
        let span = span as u32;
        let limit = i32::MAX as u32 - ((i32::MAX as u32 + 1) % span);
        loop {
            let value = self.next_int() as u32;
            if value <= limit {
                return (i64::from(min) + i64::from(value % span)) as i32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_installed_source_vstdlib_samples() {
        let floats = [
            0.41599935,
            0.09196489,
            0.7564105,
            0.5297002,
            0.9304365,
            0.38350207,
            0.653919,
            0.066842236,
            0.7226604,
            0.6711494,
            0.38341564,
            0.6316347,
        ];
        for seed in [0, 1, -1] {
            let mut stream = UniformRandomStream::new(seed);
            for expected in floats {
                assert_eq!(stream.float(0.0, 1.0), expected);
            }
            let mut stream = UniformRandomStream::new(seed);
            for expected in [7, 10, 10, 4, 8, 1, 9, 3, 4, 8, 1, 9] {
                assert_eq!(stream.int(1, 10), expected);
            }
        }
    }

    #[test]
    fn extreme_seeds_and_empty_ranges_are_safe() {
        for seed in [i32::MIN, i32::MAX] {
            let mut stream = UniformRandomStream::new(seed);
            for _ in 0..100 {
                assert!((0.0..1.0).contains(&stream.float(0.0, 1.0)));
                assert!((-10..=10).contains(&stream.int(-10, 10)));
            }
            assert_eq!(stream.int(5, 2), 5);
        }
    }

    #[test]
    fn singleton_integer_ranges_do_not_advance_the_native_random_stream() {
        let report: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/native_random_degenerate.json")).unwrap();
        for case in report["cases"].as_array().unwrap() {
            let mut stream = UniformRandomStream::new(1);
            let min = case["bounds"][0].as_i64().unwrap() as i32;
            let max = case["bounds"][1].as_i64().unwrap() as i32;
            assert_eq!(stream.int(min, max), case["first"].as_i64().unwrap() as i32);
            for expected in case["after"].as_array().unwrap() {
                assert_eq!(stream.int(0, 9), expected.as_i64().unwrap() as i32, "native range [{min}, {max}]");
            }
        }
    }
}
