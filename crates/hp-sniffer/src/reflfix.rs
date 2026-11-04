use hp_bsp::{Bsp, lump};
use hp_fs::GameFs;

pub fn strip_comments(text: &str) -> String {
    text.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join(
        "
",
    )
}

pub fn vmt_reflectivity(text: &str) -> Option<[f32; 3]> {
    let text = &strip_comments(text);
    let lower = text.to_ascii_lowercase();
    let at = lower.find("$reflectivity")?;
    let rest = &text[at + "$reflectivity".len()..];
    let open = rest.find(['[', '{'])?;
    if rest[..open].chars().any(|c| c == '\n') {
        return None;
    }
    let close = rest[open..].find([']', '}'])? + open;
    let nums: Vec<f32> = rest[open + 1..close].split_whitespace().filter_map(|t| t.parse().ok()).collect();
    match nums.as_slice() {
        [r, g, b] => Some([*r, *g, *b]),
        _ => None,
    }
}

pub fn vmt_basetexture(text: &str) -> Option<String> {
    let text = &strip_comments(text);
    let lower = text.to_ascii_lowercase();
    let at = lower.find("$basetexture")?;
    let rest = text[at + "$basetexture".len()..].trim_start_matches(|c: char| c == '"' || c.is_whitespace());
    let token: String = rest.chars().take_while(|c| !c.is_whitespace() && *c != '"' && *c != '}').collect();
    (!token.is_empty()).then_some(token)
}

pub struct Fixed {
    pub bsp: Option<Bsp>,
    pub overridden: usize,
}

pub fn apply(bsp: &Bsp, fs: &GameFs) -> Fixed {
    let none = Fixed { bsp: None, overridden: 0 };
    let (Ok(data), Ok(table), Ok(tex)) =
        (bsp.lump(lump::TEXDATA_STRING_DATA), bsp.lump(lump::TEXDATA_STRING_TABLE), bsp.lump(lump::TEXDATA))
    else {
        return none;
    };
    let mut out = tex.into_owned();
    let mut overridden = 0usize;
    for (i, rec) in out.chunks_exact_mut(32).enumerate() {
        let Some(t) = table.get(i * 4..i * 4 + 4) else { break };
        let at = u32::from_le_bytes([t[0], t[1], t[2], t[3]]) as usize;
        let Some(tail) = data.get(at..) else { continue };
        let end = tail.iter().position(|b| *b == 0).unwrap_or(tail.len());
        let name = String::from_utf8_lossy(&tail[..end]).to_ascii_lowercase();
        let Some(bytes) = fs.read(&format!("materials/{name}.vmt")) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let Some(r) = vmt_reflectivity(&text)
            .or_else(|| vmt_basetexture(&text).and_then(|t| fs.texture_info(&t)).map(|i| i.reflectivity))
        else {
            continue;
        };
        let mut raw = [0u8; 12];
        for (k, v) in r.iter().enumerate() {
            raw[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        if rec[..12] != raw {
            rec[..12].copy_from_slice(&raw);
            overridden += 1;
        }
    }
    if overridden == 0 {
        return none;
    }
    let Ok(copy) = Bsp::from_vec(bsp.bytes().to_vec()) else { return none };
    let mut edit = copy.edit();
    if edit.replace_lump(lump::TEXDATA, out).is_err() {
        return none;
    }
    Fixed { bsp: edit.to_vec().ok().and_then(|v| Bsp::from_vec(v).ok()), overridden }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reflectivity_override_is_read_from_the_vmt_text() {
        let vmt = "\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"a/b\"\n\t\"$reflectivity\" \"[0.44 0.4 0.36]\"\n}\n";
        assert_eq!(vmt_reflectivity(vmt), Some([0.44, 0.4, 0.36]));
        assert_eq!(vmt_reflectivity("\"$REFLECTIVITY\" \"{1 2 3}\""), Some([1.0, 2.0, 3.0]));
    }

    #[test]
    fn a_commented_out_override_is_not_read() {
        assert_eq!(
            vmt_reflectivity(
                "{
//\"$reflectivity\" \"[.2 .3 .2]\"
}"
            ),
            None
        );
    }

    #[test]
    fn the_base_texture_is_read_from_the_vmt_text() {
        assert_eq!(
            vmt_basetexture(
                "\"LightmappedGeneric\"
{
	\"$basetexture\" \"tools/toolshint\"
}
"
            ),
            Some("tools/toolshint".to_owned())
        );
        assert_eq!(
            vmt_basetexture(
                "\"X\"
{
}
"
            ),
            None
        );
    }

    #[test]
    fn a_vmt_without_an_override_or_with_a_malformed_one_gives_nothing() {
        assert_eq!(vmt_reflectivity("\"LightmappedGeneric\"\n{\n}\n"), None);
        assert_eq!(vmt_reflectivity("\"$reflectivity\" \"[1 2]\""), None);
        assert_eq!(vmt_reflectivity("\"$reflectivity\"\n\"x\"\n[1 2 3]"), None);
    }
}
