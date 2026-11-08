use crate::Side;

fn decimal_bits(text: &str) -> u32 {
    let text = text.trim_start().as_bytes();
    let (negative, digits) = match text.first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    let value = digits
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .fold(0u32, |value, digit| value.wrapping_mul(10).wrapping_add(u32::from(*digit - b'0')));
    if negative { value.wrapping_neg() } else { value }
}

impl Side {
    fn compile_bits(&self, key: &str) -> u32 {
        self.extra_props
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(key))
            .fold(0, |bits, (_, value)| bits | decimal_bits(value))
    }

    pub fn compile_contents(&self) -> u32 {
        self.compile_bits("contents")
    }

    pub fn compile_surface_flags(&self) -> u32 {
        self.compile_bits("flags")
    }

    pub fn add_compile_contents(&mut self, contents: u32) {
        let missing = contents & !self.compile_contents();
        if missing != 0 {
            self.extra_props.push(("contents".to_owned(), (missing as i32).to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_side_flags_preserve_signed_masks_duplicate_keys_and_roundtrips() {
        let mut side = Side {
            extra_props: vec![
                ("Contents".into(), "1".into()),
                ("contents".into(), "  +134217728 trailing".into()),
                ("contents".into(), "-2147483648".into()),
                ("flags".into(), "128".into()),
                ("FLAGS".into(), "16".into()),
            ],
            ..Default::default()
        };
        assert_eq!(side.compile_contents(), 0x8800_0001);
        assert_eq!(side.compile_surface_flags(), 0x90);
        let previous = side.extra_props.clone();
        side.add_compile_contents(crate::CONTENTS_DETAIL);
        assert_eq!(side.extra_props, previous);
        side.add_compile_contents(0x100);
        assert_eq!(side.compile_contents(), 0x8800_0101);
        let mut map = crate::Map::new_empty();
        map.world.solids.push(crate::Solid { id: 2, sides: vec![side.clone()], ..Default::default() });
        let saved = crate::parse(&crate::write(&map)).unwrap();
        assert_eq!(saved.world.solids[0].sides[0].compile_contents(), side.compile_contents());
        assert_eq!(saved.world.solids[0].sides[0].compile_surface_flags(), side.compile_surface_flags());
    }
}
