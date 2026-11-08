pub(crate) fn forward_radians(degrees: f32) -> f32 {
    (f64::from(degrees) * (std::f64::consts::PI / 180.0)) as f32
}

pub fn view(eye: [f32; 3], right: [f32; 3], up: [f32; 3]) -> [[f32; 4]; 2] {
    [right, up].map(|v| [v[0], v[1], v[2], -((eye[0] * v[0] + eye[1] * v[1]) + eye[2] * v[2])])
}

pub(crate) fn rotation(position: [f32; 3], previous: [f32; 3], view: [[f32; 4]; 2], forward: f32) -> Option<f32> {
    let transform = |p: [f32; 3]| view.map(|v| ((p[0] * v[0] + p[1] * v[1]) + p[2] * v[2]) + v[3]);
    let current = transform(position);
    if !current[0].is_finite() {
        return None;
    }
    let previous = transform(previous);
    let [dx, dy] = std::array::from_fn(|i| current[i] - previous[i]);
    Some((f64::from(dy).atan2(f64::from(dx)) + f64::from(forward)) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn array<const N: usize>(v: &Value) -> [f32; N] {
        std::array::from_fn(|i| v[i].as_f64().unwrap() as f32)
    }

    #[test]
    fn screen_velocity_angles_match_native_math_and_affine_operation_order() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/data/native_screen_velocity_angles.json")).unwrap();
        let cases = fixture["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 360);
        for (index, case) in cases.iter().enumerate() {
            let actual = rotation(
                array(&case["position"]),
                array(&case["previous"]),
                [array(&case["view"][0]), array(&case["view"][1])],
                forward_radians(case["forward"].as_f64().unwrap() as f32),
            )
            .unwrap();
            let expected = case["expected"].as_f64().unwrap() as f32;
            assert_eq!(actual.to_bits(), expected.to_bits(), "native case {index}: {actual} vs {expected}");
        }
    }
}
