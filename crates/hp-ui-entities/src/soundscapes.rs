const MAX_ITEMS: usize = 4096;

pub(crate) fn block_names(text: &[u8]) -> Vec<String> {
    hp_kv::parse_bytes(text)
        .map(|root| root.children.iter().take(MAX_ITEMS).map(|b| b.name.trim().to_ascii_lowercase()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soundscape_names_are_listed_in_lower_case() {
        let text = "\"X.Outside\"\n{\n\t\"dsp\" \"1\"\n}\n\"gm_x.cave\" { }\n";
        assert_eq!(block_names(text.as_bytes()), ["x.outside", "gm_x.cave"]);
        assert!(block_names(b"").is_empty());
    }
}
