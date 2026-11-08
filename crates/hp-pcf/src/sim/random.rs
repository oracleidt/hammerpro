const ORDER: &[u8; 8192] = include_bytes!("random_order.bin");

const fn table() -> [f32; 4096] {
    let mut values = [0.0; 4096];
    let mut index = 0;
    while index < 4096 {
        let at = index * 2;
        let integer = u16::from_le_bytes([ORDER[at], ORDER[at + 1]]) as u32;
        let millionths = (integer * 1_000_000 + 2048) / 4096;
        values[index] = millionths as f32 / 1_000_000.0;
        index += 1;
    }
    values
}
static VALUES: [f32; 4096] = table();
pub(super) fn sample(index: u32) -> f32 {
    VALUES[index as usize & 4095]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_stratum_matches_the_native_table_bit_for_bit() {
        let native: Vec<f32> =
            serde_json::from_str(include_str!("../../tests/data/random_native_samples.json")).unwrap();
        assert_eq!(native.len(), 4096);
        for (index, expected) in native.iter().enumerate() {
            assert_eq!(sample(index as u32).to_bits(), expected.to_bits(), "entry{index}");
            assert_eq!(sample(index as u32 + 4096).to_bits(), expected.to_bits());
        }
    }
}
