use super::{V3, lerp};

// Three independent lattice permutations followed by the native six-decimal value table.
const TABLES: &[u8; 1024] = include_bytes!("noise_tables.bin");

fn corner(x: usize, y: usize, z: usize) -> f32 {
    let xy = TABLES[256 + ((TABLES[x & 255] as usize + y) & 255)] as usize;
    let index = TABLES[512 + ((xy + z) & 255)] as usize;
    let value = TABLES[768 + index] as u32;
    ((value * 1_000_000 + 127) / 255) as f32 / 1_000_000.0
}

pub(super) fn scalar(position: V3) -> f32 {
    // Source rounds coordinates through the mantissa of (32768 + coordinate).
    // This also preserves its wrapping and reduced precision at large coordinates.
    let bits = position.map(|value| (32768.0 + value).to_bits() & 65535);
    let lattice = bits.map(|value| (value >> 8) as usize);
    let fraction = bits.map(|value| (value & 255) as f32 / 256.0);
    let [x, y, z] = lattice;
    let xy = [0, 1].map(|dz| {
        let row = [0, 1].map(|dy| lerp(corner(x, y + dy, z + dz), corner(x + 1, y + dy, z + dz), fraction[0]));
        lerp(row[0], row[1], fraction[1])
    });
    (lerp(xy[0], xy[1], fraction[2]) - 0.5) * 2.0
}

pub(super) fn vector(mut position: V3) -> V3 {
    let x = scalar(position);
    for (value, offset) in position.iter_mut().zip([100000.5, 300000.25, 9000001.0]) {
        *value += offset;
    }
    let y = scalar(position);
    for (value, offset) in position.iter_mut().zip([110000.25, 310000.75, 9100000.0]) {
        *value += offset;
    }
    [x, y, scalar(position)]
}

pub(super) fn map(value: f32, min: f32, max: f32, dt: Option<f32>) -> f32 {
    let mut amplitude = (max - min) * 0.5;
    let mut center = min + amplitude;
    if let Some(dt) = dt {
        amplitude *= dt;
        center *= dt;
    }
    value * amplitude + center
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_lattice_interpolation_matches_all_measured_bits() {
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../tests/data/noise_native_math.json")).unwrap();
        assert_eq!(cases.len(), 1004);
        for case in cases {
            let position = std::array::from_fn(|axis| case["position"][axis].as_f64().unwrap() as f32);
            let expected = case["expected"].as_f64().unwrap() as f32;
            assert_eq!(scalar(position).to_bits(), expected.to_bits(), "{position:?}");
        }
    }
}
