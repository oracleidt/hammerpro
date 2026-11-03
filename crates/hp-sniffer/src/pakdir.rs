use std::path::Path;

use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

const SIGNATURE: &[u8; 4] = b"PK\x01\x02";
const FIXED: usize = 46;
const MAX_NAME: usize = 260;
const MAX_TAIL: usize = 64;
const MIN_RUN: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PakEntry {
    pub name: String,
    pub crc32: u32,
    pub size: u32,
    pub compressed: u32,
    pub method: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Identical,
    Differs,
    Absent,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Identical => "identical to the pakfile entry",
            Status::Differs => "present, differs from the pakfile entry",
            Status::Absent => "not recovered",
        }
    }
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

pub fn parse_entry(d: &[u8]) -> Option<(PakEntry, usize)> {
    if d.len() < FIXED || &d[0..4] != SIGNATURE {
        return None;
    }
    let method = u16_at(d, 10);
    let (name_len, extra, comment) =
        (usize::from(u16_at(d, 28)), usize::from(u16_at(d, 30)), usize::from(u16_at(d, 32)));
    if !matches!(method, 0 | 8) || name_len == 0 || name_len > MAX_NAME || extra > MAX_TAIL || comment > MAX_TAIL {
        return None;
    }
    let name = d.get(FIXED..FIXED + name_len)?;
    if !name.iter().all(|b| (0x20..0x7f).contains(b)) {
        return None;
    }
    let entry = PakEntry {
        name: String::from_utf8_lossy(name).replace(char::from(92u8), "/"),
        crc32: u32_at(d, 16),
        compressed: u32_at(d, 20),
        size: u32_at(d, 24),
        method,
    };
    Some((entry, FIXED + name_len + extra + comment))
}

pub fn find_directory<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> Vec<PakEntry> {
    let finder = memchr::memmem::Finder::new(SIGNATURE).into_owned();
    let scan = ScanOptions { overlap: FIXED + MAX_NAME + 2 * MAX_TAIL, ..*opts };
    let mut found: Vec<(u64, PakEntry, usize)> = par_chunks(src, &scan, |base, data, own| {
        let mut out = Vec::new();
        for j in finder.find_iter(data) {
            if j >= own {
                break;
            }
            if let Some((e, total)) = parse_entry(&data[j..]) {
                out.push((base + j as u64, e, total));
            }
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    found.sort_by_key(|f| f.0);
    found.dedup_by_key(|f| f.0);
    let mut best: Vec<PakEntry> = Vec::new();
    let mut run: Vec<PakEntry> = Vec::new();
    let mut next = 0u64;
    for (addr, entry, total) in found {
        if addr != next && !run.is_empty() {
            if run.len() >= MIN_RUN && run.len() > best.len() {
                best = std::mem::take(&mut run);
            }
            run.clear();
        }
        run.push(entry);
        next = addr + total as u64;
    }
    if run.len() >= MIN_RUN && run.len() > best.len() {
        best = run;
    }
    best
}

pub fn compare(entries: &[PakEntry], read: impl Fn(&str) -> Option<Vec<u8>>) -> Vec<(PakEntry, Status)> {
    entries
        .iter()
        .map(|e| {
            let status = match read(&e.name) {
                None => Status::Absent,
                Some(bytes) => {
                    if bytes.len() == e.size as usize && crc32fast::hash(&bytes) == e.crc32 {
                        Status::Identical
                    } else {
                        Status::Differs
                    }
                }
            };
            (e.clone(), status)
        })
        .collect()
}

pub fn compare_in(entries: &[PakEntry], dest: &Path) -> Vec<(PakEntry, Status)> {
    compare(entries, |name| hp_decompile::pak::safe_join(dest, name).and_then(|p| std::fs::read(p).ok()))
}

pub fn listing_text(rows: &[(PakEntry, Status)]) -> String {
    let mut lines: Vec<String> = rows
        .iter()
        .map(|(e, s)| format!("{}\t{} bytes\tcrc32 {:08x}\t{}", e.name, e.size, e.crc32, s.label()))
        .collect();
    lines.sort();
    lines.join("\r\n") + "\r\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    fn entry_bytes(name: &str, crc: u32, size: u32, method: u16) -> Vec<u8> {
        let mut d = vec![0u8; FIXED];
        d[0..4].copy_from_slice(SIGNATURE);
        d[10..12].copy_from_slice(&method.to_le_bytes());
        d[16..20].copy_from_slice(&crc.to_le_bytes());
        d[20..24].copy_from_slice(&size.to_le_bytes());
        d[24..28].copy_from_slice(&size.to_le_bytes());
        d[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
        d.extend_from_slice(name.as_bytes());
        d
    }

    #[test]
    fn a_central_directory_in_memory_is_listed_with_sizes_and_checksums() {
        let mut dir = Vec::new();
        for (n, c, s) in [
            ("materials/a/b.vmt", 0x1111u32, 58u32),
            ("models/a/c.mdl", 0x2222, 7980),
            ("models/a/c.vvd", 0x3333, 36784),
        ] {
            dir.extend(entry_bytes(n, c, s, 0));
        }
        let mut m = VecMem::default();
        m.add(0x1000_0000, [vec![0x41u8; 40], dir, vec![0x50, 0x4b, 5, 6, 0, 0]].concat());
        m.add(0x2000_0000, entry_bytes("lonely/entry.txt", 1, 2, 0));
        let found = find_directory(&m, &ScanOptions::default());
        assert_eq!(found.len(), 3);
        assert_eq!((found[1].name.as_str(), found[1].crc32, found[1].size), ("models/a/c.mdl", 0x2222, 7980));
    }

    #[test]
    fn broken_entries_are_refused() {
        assert!(parse_entry(&entry_bytes("a/b", 1, 2, 14)).is_none());
        assert!(parse_entry(&entry_bytes("", 1, 2, 0)).is_none());
        assert!(parse_entry(&entry_bytes("bad\u{7f}name", 1, 2, 0)).is_none());
        assert!(parse_entry(&[0u8; 10]).is_none());
        let (e, total) = parse_entry(&entry_bytes("Models\\X.mdl", 5, 6, 8)).expect("entry");
        assert_eq!((e.name.as_str(), total), ("Models/X.mdl", FIXED + 12));
    }

    #[test]
    fn recovered_files_are_checked_against_the_directory_by_size_and_checksum() {
        let entries = vec![
            PakEntry {
                name: "models/a.mdl".to_owned(),
                crc32: crc32fast::hash(b"hello"),
                size: 5,
                compressed: 5,
                method: 0,
            },
            PakEntry {
                name: "models/b.mdl".to_owned(),
                crc32: crc32fast::hash(b"world"),
                size: 5,
                compressed: 5,
                method: 0,
            },
            PakEntry { name: "models/c.mdl".to_owned(), crc32: 1, size: 1, compressed: 1, method: 0 },
        ];
        let rows = compare(&entries, |name| match name {
            "models/a.mdl" => Some(b"hello".to_vec()),
            "models/b.mdl" => Some(b"world!".to_vec()),
            _ => None,
        });
        assert_eq!(
            rows.iter().map(|r| r.1).collect::<Vec<_>>(),
            vec![Status::Identical, Status::Differs, Status::Absent]
        );
        let text = listing_text(&rows);
        assert!(text.contains("models/a.mdl\t5 bytes"));
        assert!(text.contains("not recovered"));
    }
}
