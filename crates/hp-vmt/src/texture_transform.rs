/// The authored Source texture transform, before shader-specific UV routing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureTransform {
    pub center: [f32; 2],
    pub scale: [f32; 2],
    pub rotate: f32,
    pub translate: [f32; 2],
}

impl Default for TextureTransform {
    fn default() -> Self {
        Self { center: [0.5; 2], scale: [1.0; 2], rotate: 0.0, translate: [0.0; 2] }
    }
}

impl TextureTransform {
    /// Source requires the full, case-sensitive center/scale/rotate/translate
    /// sequence. An incomplete expression leaves the native matrix at identity.
    pub fn parse(text: &str) -> Self {
        let mut input = Input(text);
        let Some(parsed) = (|| {
            input.word("center")?;
            let center = [input.number()?, input.number()?];
            input.word("scale")?;
            let scale = [input.number()?, input.number()?];
            input.word("rotate")?;
            let rotate = input.number()?;
            input.word("translate")?;
            let translate = [input.number()?, input.number()?];
            Some(Self { center, scale, rotate, translate })
        })() else {
            return Self::default();
        };
        parsed
    }

    /// Two affine rows, including translation. Singular transforms are valid.
    pub fn matrix(self) -> [[f32; 3]; 2] {
        let (sin, cos) = self.rotate.to_radians().sin_cos();
        let a = cos * self.scale[0];
        let b = -sin * self.scale[1];
        let c = sin * self.scale[0];
        let d = cos * self.scale[1];
        let x = (self.center[0] + self.translate[0]) - (a * self.center[0] + b * self.center[1]);
        let y = (self.center[1] + self.translate[1]) - (c * self.center[0] + d * self.center[1]);
        [[a, b, x], [c, d, y]]
    }
}

struct Input<'a>(&'a str);

impl Input<'_> {
    fn trim(&mut self) {
        self.0 = self.0.trim_start_matches(|c: char| c.is_ascii_whitespace());
    }

    fn word(&mut self, word: &str) -> Option<()> {
        self.trim();
        self.0 = self.0.strip_prefix(word)?;
        Some(())
    }

    fn number(&mut self) -> Option<f32> {
        self.trim();
        let bytes = self.0.as_bytes();
        let mut at = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
        let mut digits = 0;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
            digits += 1;
        }
        if bytes.get(at) == Some(&b'.') {
            at += 1;
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
                digits += 1;
            }
        }
        if digits == 0 {
            return None;
        }
        if matches!(bytes.get(at), Some(b'e' | b'E')) {
            at += 1;
            if matches!(bytes.get(at), Some(b'+' | b'-')) {
                at += 1;
            }
            let start = at;
            while bytes.get(at).is_some_and(u8::is_ascii_digit) {
                at += 1;
            }
            if at == start {
                return None;
            }
        }
        let value = self.0[..at].parse::<f32>().ok().filter(|value| value.is_finite())?;
        self.0 = &self.0[at..];
        Some(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_and_affine_rows_match_native_material_matrices() {
        let report: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/native_texture_transform.json")).unwrap();
        let cases = report["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 41);
        for case in cases {
            let text = case["input"].as_str().unwrap();
            let rows = TextureTransform::parse(text).matrix();
            for row in 0..2 {
                for (out, source) in [0, 1, 3].into_iter().enumerate() {
                    let expected: f32 = case["matrix"][row][source].as_str().unwrap().parse().unwrap();
                    let error = (rows[row][out] - expected).abs();
                    assert!(
                        error <= 2e-6 * expected.abs().max(1.0),
                        "{text:?} row{row} col{source}: {} vs{expected}",
                        rows[row][out]
                    );
                }
            }
        }
        assert_eq!(TextureTransform::parse("scale 2 3"), TextureTransform::default());
        let singular = TextureTransform::parse("center .5 .5 scale 0 2 rotate 0 translate 0 0");
        assert_eq!(singular.matrix(), [[0.0, -0.0, 0.5], [0.0, 2.0, -0.5]]);
    }
}
