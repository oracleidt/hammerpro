use std::collections::HashMap;

use memchr::memmem;

use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

pub const RECV_PROP_SIZE: usize = 96;
pub const RECV_TYPE_DATATABLE: i32 = 6;
const MAX_IMAGE: usize = 256 << 20;
const MAX_PROPS: u32 = 4000;
const ITEM_STRIDE: usize = 48;
const MAX_TABLE_ENTRIES: usize = 1 << 17;
const MAX_ARRAY_ENTRIES: usize = 1 << 14;
const TABLE_HEADER: usize = 0xc0;

pub fn rd32(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

pub fn rd64(d: &[u8], at: usize) -> Option<u64> {
    d.get(at..at + 8).map(|b| u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
}

pub fn user_pointer(p: u64) -> bool {
    (0x10000..0x7fff_ffff_ffff).contains(&p)
}

pub struct Image {
    pub name: String,
    pub base: u64,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeHeader {
    pub size_of_image: u32,
    pub export_rva: u32,
}

pub fn parse_pe_header(hdr: &[u8]) -> Option<PeHeader> {
    if hdr.get(0..2)? != b"MZ" {
        return None;
    }
    let e = rd32(hdr, 0x3c)? as usize;
    if hdr.get(e..e + 4)? != b"PE\0\0" {
        return None;
    }
    let optional = e + 24;
    if rd32(hdr, optional).map(|m| m & 0xffff) != Some(0x20b) {
        return None;
    }
    let size_of_image = rd32(hdr, optional + 56)?;
    let export_rva = rd32(hdr, optional + 112)?;
    Some(PeHeader { size_of_image, export_rva })
}

pub fn read_c_string<S: MemSource + ?Sized>(src: &S, addr: u64, max: usize) -> Option<String> {
    if !user_pointer(addr) {
        return None;
    }
    let mut out = Vec::new();
    let mut at = addr;
    while out.len() < max {
        let room = 0x1000 - (at & 0xfff) as usize;
        let want = room.min(max - out.len());
        let chunk = src.read_vec(at, want);
        if chunk.is_empty() {
            return None;
        }
        if let Some(end) = chunk.iter().position(|&b| b == 0) {
            out.extend_from_slice(&chunk[..end]);
            return Some(String::from_utf8_lossy(&out).into_owned());
        }
        out.extend_from_slice(&chunk);
        if chunk.len() < want {
            return None;
        }
        at += chunk.len() as u64;
    }
    None
}

pub fn read_span<S: MemSource + ?Sized>(src: &S, addr: u64, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut at = addr;
    while out.len() < len {
        let room = 0x10000 - (at & 0xffff) as usize;
        let want = room.min(len - out.len());
        let chunk = src.read_vec(at, want);
        let got = chunk.len();
        out.extend_from_slice(&chunk);
        if got < want {
            break;
        }
        at += got as u64;
    }
    out
}

pub fn printable(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| (0x20..0x7f).contains(&b))
}

pub fn find_module<S: MemSource + ?Sized>(src: &S, name: &str) -> Option<Image> {
    let regions = src.regions();
    for r in regions.iter().filter(|r| r.image) {
        let hdr = src.read_vec(r.base, 0x400);
        let Some(pe) = parse_pe_header(&hdr) else { continue };
        if pe.export_rva == 0 || pe.size_of_image == 0 || pe.size_of_image as usize > MAX_IMAGE {
            continue;
        }
        let export = src.read_vec(r.base + pe.export_rva as u64, 0x20);
        let Some(name_rva) = rd32(&export, 12) else { continue };
        let Some(dll) = read_c_string(src, r.base + name_rva as u64, 64) else { continue };
        if !dll.eq_ignore_ascii_case(name) {
            continue;
        }
        let size = pe.size_of_image as usize;
        let mut bytes = vec![0u8; size];
        for part in regions.iter().filter(|p| p.base < r.base + size as u64 && p.base + p.size > r.base) {
            let from = part.base.max(r.base);
            let to = (part.base + part.size).min(r.base + size as u64);
            let off = (from - r.base) as usize;
            src.read(from, &mut bytes[off..off + (to - from) as usize]);
        }
        return Some(Image { name: dll, base: r.base, bytes });
    }
    None
}

impl Image {
    pub fn from_bytes(name: &str, base: u64, bytes: Vec<u8>) -> Image {
        Image { name: name.to_owned(), base, bytes }
    }

    pub fn contains(&self, addr: u64) -> bool {
        addr >= self.base && addr < self.base + self.bytes.len() as u64
    }

    pub fn find_cstr(&self, text: &str) -> Option<u64> {
        let mut needle = text.as_bytes().to_vec();
        needle.push(0);
        let mut from = 0usize;
        while let Some(p) = memmem::find(&self.bytes[from..], &needle) {
            let at = from + p;
            if at == 0 || self.bytes[at - 1] == 0 {
                return Some(self.base + at as u64);
            }
            from = at + 1;
        }
        None
    }

    pub fn qword_positions(&self, value: u64) -> Vec<usize> {
        let needle = value.to_le_bytes();
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(p) = memmem::find(&self.bytes[from..], &needle) {
            let at = from + p;
            if at.is_multiple_of(8) {
                out.push(at);
            }
            from = at + 1;
        }
        out
    }
}

#[derive(Clone, Debug, Default)]
pub struct Rtti {
    pub vtables: HashMap<u64, String>,
    pub bases: HashMap<String, Vec<String>>,
}

impl Rtti {
    pub fn vtables_of(&self, class: &str) -> Vec<u64> {
        let mut v: Vec<u64> = self.vtables.iter().filter(|(_, n)| n.as_str() == class).map(|(a, _)| *a).collect();
        v.sort_unstable();
        v
    }

    pub fn derives(&self, class: &str, ancestor: &str) -> bool {
        class == ancestor || self.bases.get(class).is_some_and(|b| b.iter().any(|x| x == ancestor))
    }
}

fn clean_type_name(raw: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(raw).ok()?;
    let s = s.strip_prefix(".?AV").or_else(|| s.strip_prefix(".?AU"))?;
    let s = s.strip_suffix("@@").unwrap_or(s);
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_graphic()) {
        return None;
    }
    Some(s.to_owned())
}

pub fn parse_rtti(img: &Image) -> Rtti {
    let d = &img.bytes;
    let mut td: HashMap<u32, String> = HashMap::new();
    let mut from = 0usize;
    while let Some(p) = memmem::find(&d[from..], b".?A") {
        let at = from + p;
        from = at + 3;
        if at < 16 || !matches!(d.get(at + 3), Some(b'V' | b'U')) {
            continue;
        }
        let end = d[at..].iter().take(300).position(|&b| b == 0).map(|e| at + e);
        let Some(end) = end else { continue };
        if let Some(name) = clean_type_name(&d[at..end]) {
            td.insert((at - 16) as u32, name);
        }
    }
    let mut cols: HashMap<u64, (String, u32, u32)> = HashMap::new();
    let mut at = 12usize;
    while at + 12 <= d.len() {
        if let Some(v) = rd32(d, at)
            && let Some(name) = td.get(&v)
        {
            let col = at - 12;
            if rd32(d, col) == Some(1) && rd32(d, col + 20) == Some(col as u32) {
                let off = rd32(d, col + 4).unwrap_or(u32::MAX);
                let chd = rd32(d, col + 16).unwrap_or(0);
                cols.insert(img.base + col as u64, (name.clone(), off, chd));
            }
        }
        at += 4;
    }
    let mut rtti = Rtti::default();
    let mut at = 0usize;
    while at + 16 <= d.len() {
        if let Some(v) = rd64(d, at)
            && let Some((name, off, chd)) = cols.get(&v)
            && *off == 0
        {
            rtti.vtables.insert(img.base + at as u64 + 8, name.clone());
            if !rtti.bases.contains_key(name) {
                let mut list = Vec::new();
                let count = rd32(d, *chd as usize + 8).unwrap_or(0).min(256) as usize;
                let arr = rd32(d, *chd as usize + 12).unwrap_or(0) as usize;
                for i in 0..count {
                    let bcd = rd32(d, arr + 4 * i).unwrap_or(0) as usize;
                    if let Some(t) = rd32(d, bcd).and_then(|t| td.get(&t)) {
                        list.push(t.clone());
                    }
                }
                rtti.bases.insert(name.clone(), list);
            }
        }
        at += 8;
    }
    rtti
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecvProp {
    pub name: String,
    pub kind: i32,
    pub offset: i32,
    pub table: u64,
}

pub fn find_recv_table<S: MemSource + ?Sized>(src: &S, img: &Image, name: &str) -> Option<u64> {
    let s = img.find_cstr(name)?;
    for at in img.qword_positions(s) {
        if at < 24 {
            continue;
        }
        let table = img.base + (at - 24) as u64;
        let head = src.read_vec(table, 16);
        let (Some(props), Some(n)) = (rd64(&head, 0), rd32(&head, 8)) else { continue };
        if user_pointer(props) && n > 0 && n < MAX_PROPS {
            return Some(table);
        }
    }
    None
}

pub fn decode_recv_props<S: MemSource + ?Sized>(src: &S, table: u64) -> Vec<RecvProp> {
    let head = src.read_vec(table, 32);
    let (Some(props), Some(n)) = (rd64(&head, 0), rd32(&head, 8)) else { return Vec::new() };
    if !user_pointer(props) || n == 0 || n > MAX_PROPS {
        return Vec::new();
    }
    let raw = read_span(src, props, n as usize * RECV_PROP_SIZE);
    let mut out = Vec::new();
    for k in 0..(raw.len() / RECV_PROP_SIZE) {
        let rec = &raw[k * RECV_PROP_SIZE..(k + 1) * RECV_PROP_SIZE];
        let name_ptr = rd64(rec, 0).unwrap_or(0);
        let kind = rd32(rec, 8).unwrap_or(u32::MAX) as i32;
        let table = rd64(rec, 64).unwrap_or(0);
        let offset = rd32(rec, 72).unwrap_or(0) as i32;
        let Some(name) = read_c_string(src, name_ptr, 96) else { continue };
        out.push(RecvProp { name, kind, offset, table });
    }
    out
}

pub fn recv_offset<S: MemSource + ?Sized>(src: &S, table: u64, prop: &str) -> Option<i32> {
    fn walk<S: MemSource + ?Sized>(src: &S, table: u64, prop: &str, depth: usize) -> Option<i32> {
        if depth > 8 {
            return None;
        }
        let props = decode_recv_props(src, table);
        if let Some(p) = props.iter().find(|p| p.name == prop && p.kind != RECV_TYPE_DATATABLE) {
            return Some(p.offset);
        }
        for p in props.iter().filter(|p| p.kind == RECV_TYPE_DATATABLE && p.name == "baseclass") {
            if let Some(o) = walk(src, p.table, prop, depth + 1) {
                return Some(o + p.offset);
            }
        }
        None
    }
    walk(src, table, prop, 0)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StringTable {
    pub name: String,
    pub items: Vec<Option<String>>,
}

impl StringTable {
    pub fn get(&self, index: usize) -> Option<&str> {
        self.items.get(index).and_then(|s| s.as_deref())
    }
}

pub fn decode_table_items<S: MemSource + ?Sized>(src: &S, header: &[u8]) -> Option<Vec<Option<String>>> {
    let max = rd32(header, 0x18)? as usize;
    if max == 0 || max > MAX_TABLE_ENTRIES {
        return None;
    }
    let probe_count = 16usize;
    let mut best: Option<(usize, u64)> = None;
    for field in (0x18..TABLE_HEADER.saturating_sub(8)).step_by(8) {
        let Some(ptr) = rd64(header, field) else { continue };
        if !user_pointer(ptr) {
            continue;
        }
        let probe = read_span(src, ptr, ITEM_STRIDE * probe_count);
        if probe.len() < ITEM_STRIDE * probe_count {
            continue;
        }
        let good = (0..probe_count)
            .filter(|i| {
                rd64(&probe, i * ITEM_STRIDE).and_then(|p| read_c_string(src, p, 260)).is_some_and(|s| printable(&s))
            })
            .count();
        if good >= 2 && best.is_none_or(|(g, _)| good > g) {
            best = Some((good, ptr));
        }
    }
    let (_, ptr) = best?;
    let raw = read_span(src, ptr, max * ITEM_STRIDE);
    let mut items = Vec::with_capacity(max);
    for i in 0..(raw.len() / ITEM_STRIDE) {
        let s = rd64(&raw, i * ITEM_STRIDE).and_then(|p| read_c_string(src, p, 260)).filter(|s| printable(s));
        items.push(s);
    }
    Some(items)
}

pub fn find_string_tables<S: MemSource + ?Sized>(src: &S, engine_rtti: &Rtti, wanted: &[&str]) -> Vec<StringTable> {
    let vts = engine_rtti.vtables_of("CNetworkStringTable");
    if vts.is_empty() {
        return Vec::new();
    }
    let opts = ScanOptions::default();
    let hits: Vec<Vec<u64>> = par_chunks(src, &opts, |base, buf, own| {
        let mut out = Vec::new();
        let mut i = ((8 - (base & 7)) & 7) as usize;
        while i + 8 <= own {
            if vts.contains(&u64::from_le_bytes(buf[i..i + 8].try_into().expect("8 bytes"))) {
                out.push(base + i as u64);
            }
            i += 8;
        }
        out
    });
    let mut tables: Vec<StringTable> = Vec::new();
    for addr in hits.into_iter().flatten() {
        let header = src.read_vec(addr, TABLE_HEADER);
        if header.len() < TABLE_HEADER {
            continue;
        }
        let Some(name) = rd64(&header, 16).and_then(|p| read_c_string(src, p, 64)) else { continue };
        if !wanted.iter().any(|w| w.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let Some(items) = decode_table_items(src, &header) else { continue };
        if items.iter().all(|i| i.is_none()) {
            continue;
        }
        tables.push(StringTable { name, items });
    }
    tables
}

pub fn scan_item_arrays<S: MemSource + ?Sized>(
    src: &S,
    target: &str,
    index: usize,
    accept: &dyn Fn(&[Option<String>]) -> bool,
) -> Vec<Vec<Option<String>>> {
    let opts = ScanOptions::default();
    let mut needle = vec![0u8];
    needle.extend_from_slice(target.as_bytes());
    needle.push(0);
    let strings: Vec<u64> = par_chunks(src, &opts, |base, buf, own| {
        let hay = &buf[..own.min(buf.len())];
        memmem::find_iter(hay, &needle).map(|p| base + p as u64 + 1).collect::<Vec<u64>>()
    })
    .into_iter()
    .flatten()
    .collect();
    if strings.is_empty() {
        return Vec::new();
    }
    let wanted: std::collections::HashSet<u64> = strings.into_iter().collect();
    let mut slots: Vec<u64> = par_chunks(src, &opts, |base, buf, own| {
        let mut out = Vec::new();
        let mut i = ((8 - (base & 7)) & 7) as usize;
        while i + 8 <= own {
            if wanted.contains(&u64::from_le_bytes(buf[i..i + 8].try_into().expect("8 bytes"))) {
                out.push(base + i as u64);
            }
            i += 8;
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    slots.sort_unstable();
    let mut found: Vec<Vec<Option<String>>> = Vec::new();
    for slot in slots {
        let Some(first) = slot.checked_sub((index * ITEM_STRIDE) as u64) else { continue };
        let raw = read_span(src, first, MAX_ARRAY_ENTRIES * ITEM_STRIDE);
        let mut items: Vec<Option<String>> = Vec::new();
        let mut blank_run = 0usize;
        for i in 0..(raw.len() / ITEM_STRIDE) {
            let s = rd64(&raw, i * ITEM_STRIDE).and_then(|p| read_c_string(src, p, 260)).filter(|s| printable(s));
            blank_run = if s.is_some() { 0 } else { blank_run + 1 };
            items.push(s);
            if blank_run > 64 && i > index {
                break;
            }
        }
        while items.last().is_some_and(|i| i.is_none()) {
            items.pop();
        }
        if items.get(index).and_then(|i| i.as_deref()) != Some(target) || !accept(&items) {
            continue;
        }
        if !found.contains(&items) {
            found.push(items);
        }
    }
    found
}

#[cfg(test)]
pub mod testkit {
    use super::*;

    pub fn put32(d: &mut [u8], at: usize, v: u32) {
        d[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }

    pub fn put64(d: &mut [u8], at: usize, v: u64) {
        d[at..at + 8].copy_from_slice(&v.to_le_bytes());
    }

    pub fn put_str(d: &mut [u8], at: usize, s: &str) {
        d[at..at + s.len()].copy_from_slice(s.as_bytes());
        d[at + s.len()] = 0;
    }

    pub struct ClassSpec<'a> {
        pub name: &'a str,
        pub bases: &'a [&'a str],
        pub vtable_rva: usize,
    }

    pub fn rtti_image(base: u64, specs: &[ClassSpec<'_>]) -> Image {
        let mut d = vec![0u8; 0x10000];
        let mut td_rva: HashMap<String, usize> = HashMap::new();
        let mut cursor = 0x1000usize;
        let mut names: Vec<&str> = Vec::new();
        for s in specs {
            if !names.contains(&s.name) {
                names.push(s.name);
            }
            for b in s.bases {
                if !names.contains(b) {
                    names.push(b);
                }
            }
        }
        for n in &names {
            let full = format!(".?AV{n}@@");
            put_str(&mut d, cursor + 16, &full);
            td_rva.insert((*n).to_owned(), cursor);
            cursor += 16 + full.len() + 8;
            cursor = (cursor + 15) & !15;
        }
        let mut bcd_rva: HashMap<String, usize> = HashMap::new();
        for n in &names {
            put32(&mut d, cursor, td_rva[*n] as u32);
            bcd_rva.insert((*n).to_owned(), cursor);
            cursor += 32;
        }
        for s in specs {
            let arr = cursor;
            let mut all = vec![s.name];
            all.extend_from_slice(s.bases);
            for (i, b) in all.iter().enumerate() {
                put32(&mut d, arr + 4 * i, bcd_rva[*b] as u32);
            }
            cursor += 4 * all.len() + 4;
            cursor = (cursor + 15) & !15;
            let chd = cursor;
            put32(&mut d, chd + 8, all.len() as u32);
            put32(&mut d, chd + 12, arr as u32);
            cursor += 32;
            let col = cursor;
            put32(&mut d, col, 1);
            put32(&mut d, col + 4, 0);
            put32(&mut d, col + 12, td_rva[s.name] as u32);
            put32(&mut d, col + 16, chd as u32);
            put32(&mut d, col + 20, col as u32);
            cursor += 32;
            put64(&mut d, s.vtable_rva, base + col as u64);
        }
        Image::from_bytes("test.dll", base, d)
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;
    use crate::procmem::VecMem;

    #[test]
    fn a_pe_header_gives_the_image_size_and_export_directory() {
        let mut h = vec![0u8; 0x400];
        h[0] = b'M';
        h[1] = b'Z';
        put32(&mut h, 0x3c, 0x80);
        h[0x80..0x84].copy_from_slice(b"PE\0\0");
        put32(&mut h, 0x80 + 24, 0x20b);
        put32(&mut h, 0x80 + 24 + 56, 0x123000);
        put32(&mut h, 0x80 + 24 + 112, 0x4567);
        assert_eq!(parse_pe_header(&h), Some(PeHeader { size_of_image: 0x123000, export_rva: 0x4567 }));
        h[0] = b'X';
        assert_eq!(parse_pe_header(&h), None);
    }

    #[test]
    fn rtti_gives_vtables_and_base_classes() {
        let specs = [
            ClassSpec { name: "C_BaseEntity", bases: &[], vtable_rva: 0x8000 },
            ClassSpec { name: "C_BaseAnimating", bases: &["C_BaseEntity"], vtable_rva: 0x8100 },
            ClassSpec { name: "C_WeaponSMG1", bases: &["C_BaseAnimating", "C_BaseEntity"], vtable_rva: 0x8200 },
        ];
        let img = rtti_image(0x7ff0_0000_0000, &specs);
        let r = parse_rtti(&img);
        assert_eq!(r.vtables.get(&(0x7ff0_0000_0000 + 0x8008)).map(String::as_str), Some("C_BaseEntity"));
        assert_eq!(r.vtables_of("C_WeaponSMG1"), vec![0x7ff0_0000_0000 + 0x8208]);
        assert!(r.derives("C_WeaponSMG1", "C_BaseAnimating"));
        assert!(r.derives("C_WeaponSMG1", "C_BaseEntity"));
        assert!(!r.derives("C_BaseEntity", "C_BaseAnimating"));
    }

    fn recv_world() -> (VecMem, Image, u64) {
        let base = 0x7ff1_0000_0000u64;
        let mut img = vec![0u8; 0x4000];
        put_str(&mut img, 0x100, "DT_BaseEntity");
        put_str(&mut img, 0x120, "DT_BaseAnimating");
        put_str(&mut img, 0x140, "m_vecOrigin");
        put_str(&mut img, 0x160, "m_nSkin");
        put_str(&mut img, 0x180, "baseclass");
        let ent_props = 0x1000usize;
        let anim_props = 0x1400usize;
        let ent_table = 0x1800usize;
        let anim_table = 0x1840usize;
        put64(&mut img, ent_props, base + 0x140);
        put32(&mut img, ent_props + 8, 2);
        put32(&mut img, ent_props + 72, 1080);
        put64(&mut img, anim_props, base + 0x180);
        put32(&mut img, anim_props + 8, RECV_TYPE_DATATABLE as u32);
        put64(&mut img, anim_props + 64, base + ent_table as u64);
        put32(&mut img, anim_props + 72, 0);
        put64(&mut img, anim_props + RECV_PROP_SIZE, base + 0x160);
        put32(&mut img, anim_props + RECV_PROP_SIZE + 72, 5824);
        put64(&mut img, ent_table, base + ent_props as u64);
        put32(&mut img, ent_table + 8, 1);
        put64(&mut img, ent_table + 24, base + 0x100);
        put64(&mut img, anim_table, base + anim_props as u64);
        put32(&mut img, anim_table + 8, 2);
        put64(&mut img, anim_table + 24, base + 0x120);
        let mut mem = VecMem::default();
        mem.add(base, img.clone());
        (mem, Image::from_bytes("client.dll", base, img), base)
    }

    #[test]
    fn recv_tables_are_found_by_name_and_offsets_follow_the_base_class() {
        let (mem, img, base) = recv_world();
        let ent = find_recv_table(&mem, &img, "DT_BaseEntity").expect("entity table");
        let anim = find_recv_table(&mem, &img, "DT_BaseAnimating").expect("animating table");
        assert_eq!(ent, base + 0x1800);
        assert_eq!(recv_offset(&mem, ent, "m_vecOrigin"), Some(1080));
        assert_eq!(recv_offset(&mem, anim, "m_nSkin"), Some(5824));
        assert_eq!(recv_offset(&mem, anim, "m_vecOrigin"), Some(1080));
        assert_eq!(recv_offset(&mem, ent, "m_nSkin"), None);
        assert!(find_recv_table(&mem, &img, "DT_Nothing").is_none());
    }

    #[test]
    fn string_table_items_are_read_at_a_forty_eight_byte_stride() {
        let base = 0x2000_0000u64;
        let mut m = vec![0u8; 0x10000];
        let names = ["x", "worldspawn", "func_brush", "weapon_smg1"];
        for (i, n) in names.iter().enumerate() {
            put_str(&mut m, 0x4000 + i * 32, n);
            put64(&mut m, 0x2000 + i * ITEM_STRIDE, base + 0x4000 + (i * 32) as u64);
        }
        put32(&mut m, 0x18, 8);
        put64(&mut m, 0x98, base + 0x2000);
        let mut mem = VecMem::default();
        mem.add(base, m.clone());
        let items = decode_table_items(&mem, &m[..TABLE_HEADER]).expect("items");
        assert_eq!(items[1].as_deref(), Some("worldspawn"));
        assert_eq!(items[3].as_deref(), Some("weapon_smg1"));
        assert_eq!(items.len(), 8);
        assert_eq!(items[5], None);
        let table = StringTable { name: "networkstring".into(), items };
        assert_eq!(table.get(2), Some("func_brush"));
        assert_eq!(table.get(99), None);
    }

    #[test]
    fn a_span_read_keeps_what_is_readable_before_a_hole() {
        let mut mem = VecMem::default();
        mem.add(0x1_0000_0000, vec![7u8; 0x18000]);
        let got = read_span(&mem, 0x1_0000_0000 + 0x100, 0x30000);
        assert_eq!(got.len(), 0x18000 - 0x100);
        assert!(got.iter().all(|b| *b == 7));
        assert!(read_span(&mem, 0x2_0000_0000, 64).is_empty());
    }

    #[test]
    fn an_item_array_is_found_from_one_known_string_and_its_index() {
        let base = 0x3000_0000u64;
        let mut m = vec![0u8; 0x20000];
        let names = ["x", "maps/gm_test.bsp", "*1", "*2", "models/props/a.mdl"];
        for (i, n) in names.iter().enumerate() {
            put_str(&mut m, 0x8000 + i * 32 + 1, n);
            put64(&mut m, 0x2000 + i * ITEM_STRIDE, base + 0x8000 + (i * 32 + 1) as u64);
        }
        put_str(&mut m, 0x9000 + 1, "maps/gm_test.bsp");
        let mut mem = VecMem::default();
        mem.add(base, m);
        let brushy = |i: &[Option<String>]| i.get(2).and_then(|s| s.as_deref()).is_some_and(|s| s.starts_with('*'));
        let items = scan_item_arrays(&mem, "maps/gm_test.bsp", 1, &brushy).pop().expect("array");
        assert_eq!(items.len(), 5);
        assert_eq!(items[4].as_deref(), Some("models/props/a.mdl"));
        let never = |_: &[Option<String>]| false;
        assert!(scan_item_arrays(&mem, "maps/gm_test.bsp", 1, &never).is_empty());
        assert!(scan_item_arrays(&mem, "not there", 1, &brushy).is_empty());
    }
}
