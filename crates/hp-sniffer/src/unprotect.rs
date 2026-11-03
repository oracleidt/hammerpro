use std::collections::{HashMap, HashSet};

use hp_bsp::{Bsp, lump};

pub const GARBAGE_PREFIX: &str = "hp/";
pub const MARKER_KEYS: &[&str] = &["hammerpro_protected", "hammerpro_lock", "hammerpro_lock_block"];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnprotectReport {
    pub signatures: Vec<(String, [f32; 3])>,
    pub guessed: usize,
    pub guess_ambiguous: usize,
    pub garbage_names: usize,
    pub restored: usize,
    pub unresolved: usize,
    pub watermark: Option<String>,
    pub lock_hash: Option<String>,
}

impl UnprotectReport {
    pub fn protected(&self) -> bool {
        self.garbage_names > 0 || self.watermark.is_some() || self.lock_hash.is_some()
    }
}

pub fn is_garbage_name(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    match n.strip_prefix(GARBAGE_PREFIX) {
        Some(rest) => rest.len() == 8 && rest.bytes().all(|b| b.is_ascii_hexdigit()),
        None => false,
    }
}

pub fn restore_materials(
    bsp: &Bsp,
    guess: &HashMap<String, String>,
    mut report: UnprotectReport,
) -> (Option<Bsp>, UnprotectReport) {
    let (Ok(data), Ok(table)) = (bsp.lump(lump::TEXDATA_STRING_DATA), bsp.lump(lump::TEXDATA_STRING_TABLE)) else {
        return (None, report);
    };
    let name_at = |ofs: usize| -> String {
        let rest = data.get(ofs..).unwrap_or(&[]);
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        String::from_utf8_lossy(&rest[..end]).into_owned()
    };
    let names: Vec<String> =
        table.chunks_exact(4).map(|c| name_at(u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize)).collect();
    let garbage: HashSet<String> =
        names.iter().filter(|n| is_garbage_name(n)).map(|n| n.to_ascii_lowercase()).collect();
    report.garbage_names = garbage.len();
    if garbage.is_empty() {
        return (None, report);
    }
    let mut map: HashMap<String, String> = HashMap::new();
    for (k, v) in guess {
        if garbage.contains(k) {
            map.insert(k.clone(), v.clone());
            report.guessed += 1;
        }
    }
    let mut new_data: Vec<u8> = Vec::new();
    let mut new_table: Vec<u8> = Vec::with_capacity(table.len());
    let mut offsets: HashMap<String, u32> = HashMap::new();
    let mut restored: HashSet<String> = HashSet::new();
    for n in &names {
        let lower = n.to_ascii_lowercase();
        let out = match map.get(&lower) {
            Some(o) => {
                restored.insert(lower.clone());
                o.clone()
            }
            None => n.clone(),
        };
        let at = *offsets.entry(out.clone()).or_insert_with(|| {
            let at = new_data.len() as u32;
            new_data.extend_from_slice(out.as_bytes());
            new_data.push(0);
            at
        });
        new_table.extend_from_slice(&at.to_le_bytes());
    }
    report.restored = restored.len();
    report.unresolved = garbage.iter().filter(|g| !restored.contains(*g)).count();
    if restored.is_empty() {
        return (None, report);
    }
    let mut edit = match Bsp::from_vec(bsp.bytes().to_vec()) {
        Ok(b) => b.edit(),
        Err(_) => return (None, report),
    };
    if edit.replace_lump(lump::TEXDATA_STRING_DATA, new_data).is_err()
        || edit.replace_lump(lump::TEXDATA_STRING_TABLE, new_table).is_err()
    {
        return (None, report);
    }
    let out = edit.to_vec().ok().and_then(|v| Bsp::from_vec(v).ok());
    (out, report)
}

pub fn strip_markers(entities: &mut [hp_bsp::Entity], report: &mut UnprotectReport) {
    for e in entities.iter_mut().filter(|e| e.classname().eq_ignore_ascii_case("worldspawn")) {
        for (k, v) in e.pairs.iter() {
            if k.eq_ignore_ascii_case("hammerpro_protected") {
                report.watermark = Some(v.clone());
            } else if k.eq_ignore_ascii_case("hammerpro_lock") {
                report.lock_hash = Some(v.clone());
            }
        }
        e.pairs.retain(|(k, _)| !MARKER_KEYS.iter().any(|m| k.eq_ignore_ascii_case(m)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garbage_names_are_eight_hex_digits_under_hp() {
        assert!(is_garbage_name("hp/ccff4eef"));
        assert!(is_garbage_name("HP/CCFF4EEF"));
        assert!(!is_garbage_name("hp/ccff4ee"));
        assert!(!is_garbage_name("hp/ccff4eeg"));
        assert!(!is_garbage_name("concrete/floor"));
    }

    fn strings(names: &[&str]) -> (Vec<u8>, Vec<u8>) {
        let mut data = Vec::new();
        let mut table = Vec::new();
        for n in names {
            table.extend_from_slice(&(data.len() as u32).to_le_bytes());
            data.extend_from_slice(n.as_bytes());
            data.push(0);
        }
        (data, table)
    }

    fn texture_names(b: &Bsp) -> Vec<String> {
        let data = b.lump(lump::TEXDATA_STRING_DATA).unwrap().into_owned();
        let table = b.lump(lump::TEXDATA_STRING_TABLE).unwrap().into_owned();
        table
            .chunks_exact(4)
            .map(|c| {
                let o = u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize;
                let e = data[o..].iter().position(|&x| x == 0).unwrap();
                String::from_utf8_lossy(&data[o..o + e]).into_owned()
            })
            .collect()
    }

    fn protected_sample(options: &hp_bsp::ProtectOptions) -> Bsp {
        let (data, table) = strings(&["concrete/floor01", "brick/wall02", "tools/toolsnodraw", "custom/mine"]);
        let ents = hp_bsp::write_entities(&[hp_bsp::Entity {
            pairs: vec![("classname".to_owned(), "worldspawn".to_owned())],
        }]);
        let bytes = hp_bsp::synthetic_bsp(
            20,
            &[
                (lump::ENTITIES, ents),
                (lump::TEXDATA_STRING_DATA, data),
                (lump::TEXDATA_STRING_TABLE, table),
                (lump::ORIGINAL_FACES, vec![1u8; 56]),
            ],
        );
        let plain = Bsp::from_vec(bytes).unwrap();
        let mut pak = plain.pakfile().unwrap().to_writer().unwrap();
        pak.add(
            "materials/custom/mine.vmt",
            b"\"LightmappedGeneric\"
{
	\"$basetexture\" \"custom/mine\"
}
"
            .to_vec(),
        )
        .unwrap();
        pak.add("materials/custom/mine.vtf", vec![7u8; 32]).unwrap();
        let mut edit = Bsp::from_vec(plain.bytes().to_vec()).unwrap().edit();
        edit.set_pakfile(pak.finish(hp_bsp::PakOptions::default()).unwrap()).unwrap();
        let full = Bsp::from_vec(edit.to_vec().unwrap()).unwrap();
        let (out, _) = hp_bsp::protect(&full, options).unwrap();
        Bsp::from_vec(out).unwrap()
    }

    #[test]
    fn guessed_names_fill_in_from_memory_and_markers_are_reported() {
        let options = hp_bsp::ProtectOptions {
            randomize_materials: true,
            salt: "s".into(),
            watermark: "me".into(),
            lock: Some(hp_bsp::Lock { hash: hp_bsp::lock_hash("k"), block: true }),
            ..Default::default()
        };
        let protected = protected_sample(&options);
        let scrambled = texture_names(&protected);
        let mut guess = HashMap::new();
        guess.insert(scrambled[0].to_ascii_lowercase(), "concrete/floor01".to_owned());
        let (fixed, report) = restore_materials(&protected, &guess, UnprotectReport::default());
        let names = texture_names(&fixed.expect("restored"));
        assert_eq!(names[0], "concrete/floor01");
        assert_eq!(names[1], scrambled[1]);
        assert_eq!((report.guessed, report.restored, report.unresolved), (1, 1, 2));
        let mut ents = protected.entities().unwrap();
        let mut r = UnprotectReport::default();
        strip_markers(&mut ents, &mut r);
        assert_eq!(r.watermark.as_deref(), Some("me"));
        assert_eq!(r.lock_hash, Some(hp_bsp::lock_hash("k")));
        assert!(ents[0].get("hammerpro_protected").is_none() && ents[0].get("hammerpro_lock").is_none());
    }
}
