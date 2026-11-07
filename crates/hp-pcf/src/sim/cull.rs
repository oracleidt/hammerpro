pub(super) fn lifespan_expired(clock: (f32, f32, f32)) -> bool {
    let (time, born, life) = clock;
    life <= 0.0 || life <= time - born
}

pub(super) fn compact<T: Copy, C: Copy>(
    particles: &mut Vec<T>,
    contexts: &mut Vec<C>,
    stable: bool,
    dead: impl Fn(&T) -> bool,
) {
    debug_assert_eq!(particles.len(), contexts.len());
    let count = if stable {
        let mut write = 0;
        for read in 0..particles.len() {
            if !dead(&particles[read]) {
                if write != read {
                    particles[write] = particles[read];
                    contexts[write] = contexts[read];
                }
                write += 1;
            }
        }
        write
    } else {
        let mut left = 0;
        let mut right = particles.len();
        while left < right {
            if dead(&particles[left]) {
                while right > left && dead(&particles[right - 1]) {
                    right -= 1;
                }
                if left == right {
                    break;
                }
                right -= 1;
                particles[left] = particles[right];
                contexts[left] = contexts[right];
            }
            left += 1;
        }
        right
    };
    particles.truncate(count);
    contexts.truncate(count);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survivor_order_and_pending_attributes_match_the_native_kill_driver() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/particle_kill_order_native_samples.json")).unwrap();
        assert_eq!(cases.len(), 900);
        for (index, case) in cases.iter().enumerate() {
            let count = case["count"].as_u64().unwrap() as u32;
            let killed: Vec<u32> =
                case["killed"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
            let expected: Vec<u32> =
                case["expected"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap() as u32).collect();
            let mut particles: Vec<u32> = (0..count).collect();
            let mut contexts: Vec<u32> = particles.iter().map(|v| v + 100_000).collect();
            compact(&mut particles, &mut contexts, case["stable"].as_bool().unwrap(), |id| killed.contains(id));
            assert_eq!(particles, expected, "queue{index}");
            assert_eq!(contexts, expected.iter().map(|v| v + 100_000).collect::<Vec<_>>(), "context queue{index}");
        }
    }
}
