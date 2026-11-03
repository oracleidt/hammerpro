use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

pub const PHY_HEADER: usize = 16;
pub const COLLIDE_HEADER: usize = 28;
pub const SURFACE_SIGNATURE_AT: usize = 0x2c;
pub const SURFACE_SIZE_AT: usize = 0x1c;
const COLLIDE_SURFACE_AT: u64 = 8;
const COLLIDE_DRAG_AT: u64 = 0x10;
const VCOLLIDE_SOLIDS_AT: usize = 8;
const VCOLLIDE_TEXT_AT: usize = 0x10;
const VCOLLIDE_SIZE: usize = 0x20;
const MAX_SOLIDS: usize = 128;
const MAX_SURFACE: usize = 16 << 20;
const MAX_TEXT: usize = 1 << 20;
const SURFACE_SIGNATURE: [u8; 4] = *b"IVPS";
const VPHY_ID: &[u8; 4] = b"VPHY";
const VPHY_VERSION: u16 = 0x100;

fn u64_at(d: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
}

fn user_pointer(p: u64) -> bool {
    (0x1_0000..0x7fff_ffff_ffff).contains(&p)
}

#[derive(Clone, Debug, PartialEq)]
pub struct PhySolid {
    pub surface: Vec<u8>,
    pub drag: [f32; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemVCollide {
    pub address: u64,
    pub solids: Vec<PhySolid>,
    pub text: Vec<u8>,
}

pub fn surface_len(head: &[u8]) -> Option<usize> {
    if head.len() < SURFACE_SIGNATURE_AT + 4 || head[SURFACE_SIGNATURE_AT..SURFACE_SIGNATURE_AT + 4] != SURFACE_SIGNATURE {
        return None;
    }
    let word = u32::from_le_bytes([head[SURFACE_SIZE_AT], head[SURFACE_SIZE_AT + 1], head[SURFACE_SIZE_AT + 2], head[SURFACE_SIZE_AT + 3]]);
    let n = (word >> 8) as usize;
    (n > SURFACE_SIGNATURE_AT + 4 && n <= MAX_SURFACE).then_some(n)
}

pub fn read_solid<S: MemSource + ?Sized>(src: &S, collide: u64) -> Option<PhySolid> {
    if !user_pointer(collide) {
        return None;
    }
    let head = src.read_vec(collide, 0x20);
    if head.len() != 0x20 {
        return None;
    }
    let surface = u64_at(&head, COLLIDE_SURFACE_AT as usize);
    if !user_pointer(surface) {
        return None;
    }
    let first = src.read_vec(surface, 0x40);
    let n = surface_len(&first)?;
    let mut bytes = src.read_vec(surface, n);
    if bytes.len() != n {
        return None;
    }
    crate::hubdec::clear_runtime_marks(&mut bytes);
    let mut drag = [0f32; 3];
    for (k, v) in drag.iter_mut().enumerate() {
        let at = COLLIDE_DRAG_AT as usize + k * 4;
        *v = f32::from_le_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]);
        if !v.is_finite() {
            return None;
        }
    }
    Some(PhySolid { surface: bytes, drag })
}

pub fn parse_vcollide<S: MemSource + ?Sized>(src: &S, address: u64) -> Option<MemVCollide> {
    let rec = src.read_vec(address, VCOLLIDE_SIZE);
    if rec.len() != VCOLLIDE_SIZE {
        return None;
    }
    let count = usize::from(u16::from_le_bytes([rec[0], rec[1]]) & 0x7fff);
    if !(1..=MAX_SOLIDS).contains(&count) {
        return None;
    }
    let solids_at = u64_at(&rec, VCOLLIDE_SOLIDS_AT);
    let text_at = u64_at(&rec, VCOLLIDE_TEXT_AT);
    if !user_pointer(solids_at) || !user_pointer(text_at) {
        return None;
    }
    let ptrs = src.read_vec(solids_at, count * 8);
    if ptrs.len() != count * 8 {
        return None;
    }
    let mut solids = Vec::with_capacity(count);
    for c in ptrs.chunks_exact(8) {
        solids.push(read_solid(src, u64_at(c, 0))?);
    }
    let packed = rec[1] & 0x80 != 0;
    let desc = usize::from(u16::from_le_bytes([rec[2], rec[3]]));
    let text = if packed {
        let blob = src.read_vec(text_at, desc);
        if blob.len() != desc {
            return None;
        }
        crate::hubdec::decode_packed_keys(&blob)?
    } else {
        crate::matmem::read_cstring(src, text_at, MAX_TEXT)?
    };
    if !text.trim_start().starts_with("solid") && !text.trim_start().starts_with("staticsolid") {
        return None;
    }
    let mut text = text.into_bytes();
    text.push(0);
    Some(MemVCollide { address, solids, text })
}

pub fn find_vcollides<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> Vec<MemVCollide> {
    let scan = ScanOptions { overlap: VCOLLIDE_SIZE, writable_only: true, ..*opts };
    let mut candidates: Vec<u64> = par_chunks(src, &scan, |base, data, own| {
        let mut out = Vec::new();
        let limit = own.min(data.len());
        let mut j = 0usize;
        while j + VCOLLIDE_SIZE <= data.len() && j < limit {
            let count = u16::from_le_bytes([data[j], data[j + 1]]) & 0x7fff;
            if (1..=MAX_SOLIDS as u16).contains(&count)
                && user_pointer(u64_at(data, j + VCOLLIDE_SOLIDS_AT))
                && user_pointer(u64_at(data, j + VCOLLIDE_TEXT_AT))
            {
                out.push(base + j as u64);
            }
            j += 4;
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    candidates.sort_unstable();
    candidates.dedup();
    let mut found: Vec<MemVCollide> = candidates.iter().filter_map(|a| parse_vcollide(src, *a)).collect();
    found.sort_by(|a, b| a.text.cmp(&b.text).then(a.address.cmp(&b.address)));
    found.dedup_by(|a, b| a.text == b.text && a.solids == b.solids);
    found
}

const MDL_BONE_COUNT: usize = 156;
const MDL_BONE_INDEX: usize = 160;
const BONE_RECORD: usize = 216;

pub fn mdl_bone_names(mdl: &[u8]) -> Vec<String> {
    let (Some(count), Some(index)) = (mdl.get(MDL_BONE_COUNT..MDL_BONE_COUNT + 4), mdl.get(MDL_BONE_INDEX..MDL_BONE_INDEX + 4)) else {
        return Vec::new();
    };
    let count = usize::try_from(i32::from_le_bytes([count[0], count[1], count[2], count[3]])).unwrap_or(0).min(512);
    let index = usize::try_from(i32::from_le_bytes([index[0], index[1], index[2], index[3]])).unwrap_or(0);
    let mut out = Vec::new();
    for k in 0..count {
        let at = index + k * BONE_RECORD;
        let Some(rec) = mdl.get(at..at + 4) else { break };
        let off = i32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
        let Some(start) = usize::try_from(i64::try_from(at).unwrap_or(0) + i64::from(off)).ok() else { continue };
        let Some(rest) = mdl.get(start..) else { continue };
        let end = rest.iter().position(|b| *b == 0).unwrap_or(rest.len());
        out.push(String::from_utf8_lossy(&rest[..end]).into_owned());
    }
    out
}

const MDL_SURFACE_PROP: usize = 308;
const BONE_SURFACE_PROP: usize = 176;

fn mdl_text(mdl: &[u8], at: usize, rel: i64) -> Option<String> {
    let start = usize::try_from(i64::try_from(at).ok()? + rel).ok()?;
    let rest = mdl.get(start..)?;
    let end = rest.iter().position(|b| *b == 0)?;
    (end > 0).then(|| String::from_utf8_lossy(&rest[..end]).into_owned())
}

pub fn mdl_case_names(mdl: &[u8]) -> Vec<String> {
    let mut out = mdl_bone_names(mdl);
    if let Some(rel) = mdl.get(MDL_SURFACE_PROP..MDL_SURFACE_PROP + 4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]])) {
        out.extend(mdl_text(mdl, 0, i64::from(rel)));
    }
    let count = mdl.get(MDL_BONE_COUNT..MDL_BONE_COUNT + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]).clamp(0, 512)) as usize;
    let index = mdl.get(MDL_BONE_INDEX..MDL_BONE_INDEX + 4).map_or(0, |b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]).max(0)) as usize;
    for k in 0..count {
        let at = index + k * BONE_RECORD;
        if let Some(b) = mdl.get(at + BONE_SURFACE_PROP..at + BONE_SURFACE_PROP + 4) {
            out.extend(mdl_text(mdl, at, i64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))));
        }
    }
    out
}

pub fn mdl_strings(mdl: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, b) in mdl.iter().enumerate() {
        let printable = (0x20..0x7f).contains(b);
        match (start, printable) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                if *b == 0 && i - s >= 3 {
                    out.push(String::from_utf8_lossy(&mdl[s..i]).into_owned());
                }
                start = None;
            }
            _ => {}
        }
    }
    out
}

pub fn case_table(mdl: &[u8]) -> std::collections::HashMap<String, String> {
    let mut table = std::collections::HashMap::new();
    for name in mdl_case_names(mdl).into_iter().chain(mdl_strings(mdl)) {
        table.entry(name.to_ascii_lowercase()).or_insert_with(|| name.clone());
        for part in name.split(['/', char::from(92u8)]).filter(|p| !p.is_empty()) {
            table.entry(part.to_ascii_lowercase()).or_insert_with(|| part.to_owned());
            if let Some(dot) = part.rfind('.') {
                table.entry(part[..dot].to_ascii_lowercase()).or_insert_with(|| part[..dot].to_owned());
            }
        }
    }
    table
}

pub fn restore_name_case(text: &mut [u8], table: &std::collections::HashMap<String, String>) {
    let mut at = 0usize;
    while at < text.len() {
        let line_end = text[at..].iter().position(|b| *b == 10).map_or(text.len(), |p| at + p);
        let line = &text[at..line_end];
        let keys: [&[u8]; 4] = [b"\"name\" \"", b"\"parent\" \"", b"\"rootname\" \"", b"\"surfaceprop\" \""];
        let skip = keys.iter().find(|k| line.starts_with(k)).map(|k| k.len());
        if let Some(skip) = skip {
            let value_start = at + skip;
            if let Some(q) = text[value_start..line_end].iter().rposition(|b| *b == b'"') {
                let value = String::from_utf8_lossy(&text[value_start..value_start + q]).to_ascii_lowercase();
                let restored = table.get(&value).cloned().or_else(|| {
                    value.strip_suffix("_phys").and_then(|stem| table.get(stem)).map(|s| format!("{s}_phys"))
                });
                if let Some(real) = restored.filter(|r| r.len() == q) {
                    text[value_start..value_start + q].copy_from_slice(real.as_bytes());
                }
            }
        }
        at = line_end + 1;
    }
}

pub fn rebuild_phy(solids: &[PhySolid], text: &[u8], checksum: i32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(PHY_HEADER as i32).to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&(solids.len() as i32).to_le_bytes());
    out.extend_from_slice(&checksum.to_le_bytes());
    for s in solids {
        out.extend_from_slice(&((COLLIDE_HEADER + s.surface.len()) as i32).to_le_bytes());
        out.extend_from_slice(VPHY_ID);
        out.extend_from_slice(&VPHY_VERSION.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&(s.surface.len() as i32).to_le_bytes());
        for d in s.drag {
            out.extend_from_slice(&d.to_le_bytes());
        }
        out.extend_from_slice(&0i32.to_le_bytes());
        out.extend_from_slice(&s.surface);
    }
    out.extend_from_slice(text);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    fn surface(n: usize, seed: u8) -> Vec<u8> {
        let mut s: Vec<u8> = (0..n).map(|i| (i as u8).wrapping_mul(seed)).collect();
        s[SURFACE_SIGNATURE_AT..SURFACE_SIGNATURE_AT + 4].copy_from_slice(&SURFACE_SIGNATURE);
        s[SURFACE_SIZE_AT..SURFACE_SIZE_AT + 4].copy_from_slice(&((n as u32) << 8).to_le_bytes());
        s
    }

    fn memory(solids: &[(Vec<u8>, [f32; 3])], text: &str) -> (VecMem, u64) {
        let mut m = VecMem::default();
        let mut collide_ptrs = Vec::new();
        for (k, (s, drag)) in solids.iter().enumerate() {
            let surf_at = 0x1000_0000u64 + k as u64 * 0x1_0000;
            m.add(surf_at, s.clone());
            let mut c = vec![0u8; 0x20];
            c[8..16].copy_from_slice(&surf_at.to_le_bytes());
            for (i, d) in drag.iter().enumerate() {
                c[0x10 + i * 4..0x14 + i * 4].copy_from_slice(&d.to_le_bytes());
            }
            let c_at = 0x2000_0000u64 + k as u64 * 0x100;
            m.add(c_at, c);
            collide_ptrs.extend_from_slice(&c_at.to_le_bytes());
        }
        m.add(0x3000_0000, collide_ptrs);
        let mut t = text.as_bytes().to_vec();
        t.push(0);
        m.add(0x4000_0000, t);
        let mut v = vec![0u8; VCOLLIDE_SIZE];
        v[0..2].copy_from_slice(&(solids.len() as u16).to_le_bytes());
        v[8..16].copy_from_slice(&0x3000_0000u64.to_le_bytes());
        v[16..24].copy_from_slice(&0x4000_0000u64.to_le_bytes());
        m.add(0x5000_0000, v);
        (m, 0x5000_0000)
    }

    #[test]
    fn a_physics_file_is_rebuilt_with_its_headers_and_text() {
        let solids = vec![(surface(412, 3), [1.0, 1.0, 1.0]), (surface(100, 5), [0.5, 2.0, 4.0])];
        let text = "solid {\n\"index\" \"0\"\n}\neditparams {\n}\n";
        let (m, at) = memory(&solids, text);
        let v = parse_vcollide(&m, at).expect("vcollide");
        assert_eq!(v.solids.len(), 2);
        let phy = rebuild_phy(&v.solids, &v.text, -1959602624);
        assert_eq!(&phy[0..16], &[16, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0x40, 0xd6, 0x32, 0x8b]);
        assert_eq!(i32::from_le_bytes([phy[16], phy[17], phy[18], phy[19]]), 440);
        assert_eq!(&phy[20..24], b"VPHY");
        assert_eq!(u16::from_le_bytes([phy[24], phy[25]]), 0x100);
        assert_eq!(i32::from_le_bytes([phy[28], phy[29], phy[30], phy[31]]), 412);
        let mut expected = solids[0].0.clone();
        crate::hubdec::clear_runtime_marks(&mut expected);
        assert_eq!(&phy[48..48 + 412], &expected[..]);
        let second = 20 + 440;
        assert_eq!(i32::from_le_bytes([phy[second], phy[second + 1], phy[second + 2], phy[second + 3]]), 128);
        assert!(phy.ends_with(text.as_bytes()) || phy.ends_with(&[text.as_bytes(), &[0]].concat()));
    }

    #[test]
    fn collide_data_is_found_by_its_structure() {
        let solids = vec![(surface(64, 7), [1.0, 1.0, 1.0])];
        let (mut m, _) = memory(&solids, "solid {\n}\n");
        m.add(0x6000_0000, vec![0xabu8; 4096]);
        let found = find_vcollides(&m, &ScanOptions::default());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].address, 0x5000_0000);
    }

    #[test]
    fn data_without_the_surface_signature_or_text_is_refused() {
        let mut bad = surface(64, 7);
        bad[SURFACE_SIGNATURE_AT] = 0;
        let (m, at) = memory(&[(bad, [1.0; 3])], "solid {\n}\n");
        assert!(parse_vcollide(&m, at).is_none());
        let (m, at) = memory(&[(surface(64, 7), [1.0; 3])], "no keyvalues here");
        assert!(parse_vcollide(&m, at).is_none());
        assert!(surface_len(&[0u8; 8]).is_none());
    }

    fn packed_solid() -> Vec<u8> {
        let hex = "00010000060001000c0048000100000000000000000000000000000000000000040004000000000005000000000000000d0008000200000000000000000064656661756c74002c000000000000504347de3ca84a";
        (0..hex.len() / 2).map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("hex")).collect()
    }

    #[test]
    fn packed_key_values_are_decoded_back_to_their_text_with_a_closing_nul() {
        let solids = vec![(surface(412, 3), [1.0, 1.0, 1.0])];
        let (mut m, _) = memory(&solids, "solid {\n}\n");
        let blob = packed_solid();
        let mut v = vec![0u8; VCOLLIDE_SIZE];
        let word = 1u16 | 0x8000;
        v[0..2].copy_from_slice(&word.to_le_bytes());
        v[2..4].copy_from_slice(&(blob.len() as u16).to_le_bytes());
        v[8..16].copy_from_slice(&0x3000_0000u64.to_le_bytes());
        v[16..24].copy_from_slice(&0x4100_0000u64.to_le_bytes());
        m.add(0x5100_0000, v);
        m.add(0x4100_0000, blob);
        let found = parse_vcollide(&m, 0x5100_0000).expect("vcollide");
        assert_eq!(
            String::from_utf8(found.text).expect("text"),
            "solid {\n\"index\" \"0\"\n\"mass\" \"50000.000000\"\n\"surfaceprop\" \"default\"\n\"volume\" \"5512815.000000\"\n}\n\0"
        );
    }

    #[test]
    fn the_runtime_marks_of_a_model_solid_are_cleared() {
        let mut s = surface(200, 5);
        s[32..32 + 4].copy_from_slice(&48u32.to_le_bytes());
        s[36..40].copy_from_slice(&[9, 9, 9, 9]);
        s[48..52].copy_from_slice(&0i32.to_le_bytes());
        s[48 + 27] = 4;
        let mut m = VecMem::default();
        m.add(0x1000_0000, s);
        let mut c = vec![0u8; 0x20];
        c[8..16].copy_from_slice(&0x1000_0000u64.to_le_bytes());
        m.add(0x2000_0000, c);
        let solid = read_solid(&m, 0x2000_0000).expect("solid");
        assert_eq!(&solid.surface[36..40], &[0, 0, 0, 0]);
        assert_eq!(solid.surface[48 + 27], 0);
    }

    #[test]
    fn the_case_the_engine_lowercases_is_restored_from_the_strings_of_the_model() {
        let mut mdl = vec![0u8; 400];
        let strings = ["Glass_shard01.smd", "glass", "ValveBiped.Bip01_Spine"];
        let mut at = 100usize;
        for s in strings {
            mdl[at..at + s.len()].copy_from_slice(s.as_bytes());
            at += s.len() + 1;
        }
        let table = case_table(&mdl);
        let mut text = b"solid {\n\"name\" \"glass_shard01_phys\"\n\"parent\" \"valvebiped.bip01_spine\"\n\"surfaceprop\" \"glass\"\n\"index\" \"0\"\n}\n".to_vec();
        restore_name_case(&mut text, &table);
        assert_eq!(
            String::from_utf8(text).expect("text"),
            "solid {\n\"name\" \"Glass_shard01_phys\"\n\"parent\" \"ValveBiped.Bip01_Spine\"\n\"surfaceprop\" \"glass\"\n\"index\" \"0\"\n}\n"
        );
        let mut unknown = b"\"name\" \"nothing_known\"\n".to_vec();
        restore_name_case(&mut unknown, &table);
        assert_eq!(unknown, b"\"name\" \"nothing_known\"\n".to_vec());
    }

    #[test]
    fn the_bone_names_and_surface_properties_of_a_model_give_the_case_table() {
        let mut mdl = vec![0u8; 900];
        mdl[MDL_BONE_COUNT..MDL_BONE_COUNT + 4].copy_from_slice(&1i32.to_le_bytes());
        mdl[MDL_BONE_INDEX..MDL_BONE_INDEX + 4].copy_from_slice(&300i32.to_le_bytes());
        mdl[300..304].copy_from_slice(&(500i32 - 300).to_le_bytes());
        mdl[300 + BONE_SURFACE_PROP..300 + BONE_SURFACE_PROP + 4].copy_from_slice(&(520i32 - 300).to_le_bytes());
        mdl[MDL_SURFACE_PROP..MDL_SURFACE_PROP + 4].copy_from_slice(&540i32.to_le_bytes());
        mdl[500..511].copy_from_slice(b"Weapon_Bone");
        mdl[520..530].copy_from_slice(b"Metal_Barr");
        mdl[540..547].copy_from_slice(b"Default");
        let names = mdl_case_names(&mdl);
        assert_eq!(names, vec!["Weapon_Bone".to_owned(), "Default".to_owned(), "Metal_Barr".to_owned()]);
        assert_eq!(mdl_bone_names(&mdl), vec!["Weapon_Bone".to_owned()]);
    }
}
