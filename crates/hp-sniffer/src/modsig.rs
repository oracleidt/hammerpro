use crate::procmem::MemSource;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub base: u64,
    pub size: u64,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapFlags {
    pub value: u32,
    pub inferred_low_bit: bool,
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    d.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub fn read_module<S: MemSource + ?Sized>(src: &S, base: u64) -> Option<Module> {
    let head = src.read_vec(base, 0x400);
    if head.len() < 0x100 || head.get(0..2) != Some(b"MZ") {
        return None;
    }
    let pe = u32_at(&head, 0x3c)? as usize;
    if head.get(pe..pe + 4) != Some(b"PE\0\0") {
        return None;
    }
    let optional = pe + 24;
    let magic = u16_at(&head, optional)?;
    let size = u64::from(u32_at(&head, optional + 56)?);
    let export_dir_at = match magic {
        0x20b => optional + 112,
        0x10b => optional + 96,
        _ => return None,
    };
    let export_rva = u64::from(u32_at(&head, export_dir_at)?);
    let mut name = String::new();
    if export_rva != 0 {
        let dir = src.read_vec(base + export_rva, 16);
        if dir.len() == 16 {
            let name_rva = u64::from(u32_at(&dir, 12)?);
            let raw = src.read_vec(base + name_rva, 64);
            let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
            name = String::from_utf8_lossy(&raw[..end]).to_ascii_lowercase();
        }
    }
    Some(Module { base, size, name })
}

pub fn find_modules<S: MemSource + ?Sized>(src: &S) -> Vec<Module> {
    let mut out: Vec<Module> = Vec::new();
    for r in src.regions() {
        if !r.image {
            continue;
        }
        if out.iter().any(|m| r.base >= m.base && r.base < m.base + m.size) {
            continue;
        }
        if let Some(m) = read_module(src, r.base) {
            out.push(m);
        }
    }
    out
}

const ANCHOR: [u8; 7] = [0xA8, 0x03, 0x8B, 0xC8, 0x0F, 0x95, 0x05];
const LOAD_REACH: usize = 96;
const MAX_FLAG_BITS: usize = 16;

fn resolve(wide: bool, instr_end: u64, disp: u32) -> u64 {
    if wide { instr_end.wrapping_add_signed(i64::from(disp as i32)) } else { u64::from(disp) }
}

pub fn flags_from_code(code: &[u8], code_va: u64, wide: bool) -> Vec<(usize, u64)> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = code[from..].windows(ANCHOR.len()).position(|w| w == ANCHOR) {
        let at = from + rel;
        from = at + 1;
        let before = &code[at.saturating_sub(LOAD_REACH)..at];
        let pushes_lump =
            before.windows(5).any(|w| w == [0xBA, 0x3B, 0, 0, 0]) || before.windows(2).any(|w| w == [0x6A, 0x3B]);
        if !pushes_lump {
            continue;
        }
        let mut p = at + ANCHOR.len();
        let Some(d) = u32_at(code, p) else { continue };
        p += 4;
        let mut found = vec![(0usize, resolve(wide, code_va + p as u64, d))];
        for bit in 1..MAX_FLAG_BITS {
            if code.get(p..p + 2) == Some(&[0x8B, 0xC8]) {
                p += 2;
            }
            match (code.get(p), code.get(p + 1)) {
                (Some(0xD1), Some(0xE9)) => p += 2,
                (Some(0xC1), Some(0xE9)) => p += 3,
                _ => break,
            }
            if code.get(p..p + 2) == Some(&[0xF6, 0xD1]) {
                p += 2;
            }
            if code.get(p..p + 3) != Some(&[0x80, 0xE1, 0x01]) {
                break;
            }
            p += 3;
            if code.get(p..p + 2) != Some(&[0x88, 0x0D]) {
                break;
            }
            p += 2;
            let Some(d) = u32_at(code, p) else { break };
            p += 4;
            found.push((bit, resolve(wide, code_va + p as u64, d)));
        }
        if found.len() >= 3 {
            out = found;
            break;
        }
    }
    out
}

pub fn map_flags<S: MemSource + ?Sized>(src: &S, wide: bool) -> Option<MapFlags> {
    for m in find_modules(src) {
        if !m.name.is_empty() && m.name != "engine.dll" {
            continue;
        }
        let code = src.read_vec(m.base, m.size as usize);
        if code.len() < 0x1000 {
            continue;
        }
        let globals = flags_from_code(&code, m.base, wide);
        if globals.len() < 3 {
            continue;
        }
        let mut bits = Vec::new();
        for (bit, addr) in &globals {
            let b = src.read_vec(*addr, 1);
            if b.len() != 1 || b[0] > 1 {
                bits.clear();
                break;
            }
            bits.push((*bit, b[0]));
        }
        if bits.is_empty() {
            continue;
        }
        let either = u32::from(bits.iter().find(|b| b.0 == 0).map(|b| b.1).unwrap_or(0));
        let bit1_inverted = u32::from(bits.iter().find(|b| b.0 == 1).map(|b| b.1).unwrap_or(1));
        let bit1 = 1 - bit1_inverted;
        let mut value = bit1 << 1;
        let mut inferred = false;
        if either != 0 {
            value |= 1;
            inferred = bit1 == 1;
        }
        for (bit, v) in &bits {
            if *bit >= 2 {
                value |= u32::from(*v) << *bit;
            }
        }
        return Some(MapFlags { value, inferred_low_bit: inferred });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::{Region, VecMem};

    struct Img {
        mem: VecMem,
        regions: Vec<Region>,
    }

    impl MemSource for Img {
        fn regions(&self) -> Vec<Region> {
            self.regions.clone()
        }

        fn read(&self, addr: u64, buf: &mut [u8]) -> usize {
            self.mem.read(addr, buf)
        }
    }

    fn code_for(wide: bool, va: u64, globals: &[u64]) -> Vec<u8> {
        let mut c = vec![0x90u8; 8];
        if wide {
            c.extend_from_slice(&[
                0xBA, 0x3B, 0, 0, 0, 0x48, 0x8D, 0x4C, 0x24, 0x20, 0xE8, 1, 2, 3, 4, 0x48, 0x8B, 0x44, 0x24, 0x38,
                0x8B, 0x00,
            ]);
        } else {
            c.extend_from_slice(&[
                0x6A, 0x3B, 0x8D, 0x8D, 1, 2, 3, 4, 0xE8, 1, 2, 3, 4, 0x8B, 0x85, 1, 2, 3, 4, 0x8B, 0x00,
            ]);
        }
        c.extend_from_slice(&ANCHOR);
        let put = |c: &mut Vec<u8>, g: u64| {
            let end = va + c.len() as u64 + 4;
            let d = if wide { (g as i64 - end as i64) as i32 as u32 } else { g as u32 };
            c.extend_from_slice(&d.to_le_bytes());
        };
        put(&mut c, globals[0]);
        c.extend_from_slice(&[0xD1, 0xE9, 0xF6, 0xD1, 0x80, 0xE1, 0x01, 0x88, 0x0D]);
        put(&mut c, globals[1]);
        for (k, g) in globals.iter().enumerate().skip(2) {
            c.extend_from_slice(&[0x8B, 0xC8, 0xC1, 0xE9, k as u8, 0x80, 0xE1, 0x01, 0x88, 0x0D]);
            put(&mut c, *g);
        }
        c.extend_from_slice(&[0x90; 16]);
        c
    }

    fn image(wide: bool, flags: [u8; 5]) -> Img {
        let base = if wide { 0x1_8000_0000u64 } else { 0x1000_0000u64 };
        let mut page = vec![0u8; 0x1000];
        page[0..2].copy_from_slice(b"MZ");
        page[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        page[0x80..0x84].copy_from_slice(b"PE\0\0");
        let optional = 0x80 + 24;
        page[optional..optional + 2].copy_from_slice(&(if wide { 0x20bu16 } else { 0x10bu16 }).to_le_bytes());
        page[optional + 56..optional + 60].copy_from_slice(&0x4000u32.to_le_bytes());
        let export_at = optional + if wide { 112 } else { 96 };
        page[export_at..export_at + 4].copy_from_slice(&0x300u32.to_le_bytes());
        page[0x300 + 12..0x300 + 16].copy_from_slice(&0x340u32.to_le_bytes());
        page[0x340..0x340 + 11].copy_from_slice(b"engine.dll\0");
        let globals: Vec<u64> = (0..5).map(|k| base + 0x3000 + k * 3).collect();
        let code = code_for(wide, base + 0x1000, &globals);
        let mut image = page;
        image.resize(0x4000, 0);
        image[0x1000..0x1000 + code.len()].copy_from_slice(&code);
        for (k, v) in flags.iter().enumerate() {
            image[0x3000 + k * 3] = *v;
        }
        let mut mem = VecMem::default();
        mem.add(base, image);
        Img { mem, regions: vec![Region { base, size: 0x4000, writable: false, image: true }] }
    }

    #[test]
    fn the_engine_module_is_recognised_by_its_header() {
        let img = image(true, [0; 5]);
        let mods = find_modules(&img);
        assert_eq!(mods.len(), 1);
        assert_eq!(mods[0].name, "engine.dll");
        assert_eq!(mods[0].size, 0x4000);
    }

    #[test]
    fn the_map_flags_are_rebuilt_from_the_booleans_the_engine_derives_in_both_layouts() {
        for wide in [true, false] {
            let probe = image(wide, [1, 0, 0, 0, 0]);
            let found = flags_from_code(&probe.mem.blocks[0].1, probe.regions[0].base, wide);
            assert_eq!(found.len(), 5, "{wide}: {found:x?}");
            let both = image(wide, [1, 0, 0, 0, 0]);
            let f = map_flags(&both, wide).expect("flags");
            assert_eq!(f.value, 3, "{wide}");
            assert!(f.inferred_low_bit);
            let none = image(wide, [0, 1, 0, 0, 0]);
            assert_eq!(map_flags(&none, wide).map(|f| f.value), Some(0));
            let low = image(wide, [1, 1, 0, 0, 0]);
            let f = map_flags(&low, wide).expect("flags");
            assert_eq!((f.value, f.inferred_low_bit), (1, false));
            let high = image(wide, [1, 0, 1, 0, 1]);
            assert_eq!(map_flags(&high, wide).map(|f| f.value), Some(3 | 4 | 16));
        }
    }

    #[test]
    fn code_that_is_not_the_flag_loader_gives_nothing() {
        let mut img = image(true, [1, 0, 0, 0, 0]);
        img.mem.blocks[0].1[0x1000..0x1000 + 64].fill(0x90);
        assert!(flags_from_code(&img.mem.blocks[0].1, 0x1_8000_0000, true).is_empty());
        assert!(map_flags(&img, true).is_none());
        let mut weird = image(true, [1, 0, 0, 0, 0]);
        weird.mem.blocks[0].1[0x3000] = 9;
        assert!(map_flags(&weird, true).is_none(), "a global that is not a boolean is refused");
    }
}
