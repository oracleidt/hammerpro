use ahash::AHashMap;

use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

const WINDOW: usize = 32;
const STEP: usize = 256;
const MIN_DISTINCT: usize = 8;
const MIN_KEY_DISTINCT: usize = 5;
const SIGNATURE: &[u8; 4] = b"VTF\0";
const HEADER_PEEK: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VtfBlob {
    pub address: u64,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    pub faces: u32,
    pub mips: u32,
    pub format: i32,
    pub flags: u32,
    pub bytes: Vec<u8>,
}

pub fn blob_of(head: &[u8], read: impl Fn(usize) -> Vec<u8>) -> Option<VtfBlob> {
    let info = hp_vtf::read_info(head).ok()?;
    let opts = hp_vtf::DecodeOptions {
        mip: hp_vtf::MipSelect::Largest,
        frame: info.frames.checked_sub(1)?,
        face: info.faces.checked_sub(1)?,
    };
    let len = hp_vtf::required_len(head, &opts).ok()?;
    let bytes = read(len);
    (bytes.len() == len).then_some(VtfBlob {
        address: 0,
        width: info.width,
        height: info.height,
        frames: info.frames,
        faces: info.faces,
        mips: info.mip_count,
        format: info.format as i32,
        flags: info.flags,
        bytes,
    })
}

pub fn find_blobs<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> Vec<VtfBlob> {
    let finder = memchr::memmem::Finder::new(SIGNATURE).into_owned();
    let scan = ScanOptions { overlap: HEADER_PEEK, ..*opts };
    let mut addrs: Vec<u64> = par_chunks(src, &scan, |base, data, own| {
        finder.find_iter(data).filter(|&j| j < own).map(|j| base + j as u64).collect::<Vec<u64>>()
    })
    .into_iter()
    .flatten()
    .collect();
    addrs.sort_unstable();
    addrs.dedup();
    let mut out = Vec::new();
    for a in addrs {
        let head = src.read_vec(a, HEADER_PEEK);
        if let Some(mut b) = blob_of(&head, |n| src.read_vec(a, n)) {
            b.address = a;
            out.push(b);
        }
    }
    out
}

#[derive(Clone, Debug, PartialEq)]
pub struct Coverage {
    pub name: String,
    pub windows: usize,
    pub found: usize,
    pub whole: bool,
}

impl Coverage {
    pub fn fraction(&self) -> f64 {
        if self.windows == 0 { 0.0 } else { self.found as f64 / self.windows as f64 }
    }
}

fn key_of(w: &[u8]) -> u64 {
    u64::from_le_bytes([w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7]])
}

fn slot(k: u64) -> usize {
    (k.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 40) as usize & ((1 << 24) - 1)
}

fn distinct(w: &[u8]) -> usize {
    let mut seen = [false; 256];
    let mut n = 0;
    for &b in w {
        if !seen[usize::from(b)] {
            seen[usize::from(b)] = true;
            n += 1;
        }
    }
    n
}

pub fn coverage<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    files: &[(String, Vec<u8>)],
    blobs: &[VtfBlob],
) -> Vec<Coverage> {
    let mut by_key: AHashMap<u64, Vec<(usize, usize)>> = AHashMap::new();
    let mut bloom = vec![false; 1 << 24];
    let mut totals = vec![0usize; files.len()];
    for (fi, (_, data)) in files.iter().enumerate() {
        let Some(header) = data.get(12..16).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize) else {
            continue;
        };
        let mut o = header;
        while o + WINDOW <= data.len() {
            let w = &data[o..o + WINDOW];
            if distinct(w) >= MIN_DISTINCT && distinct(&w[..8]) >= MIN_KEY_DISTINCT {
                let k = key_of(w);
                bloom[slot(k)] = true;
                by_key.entry(k).or_default().push((fi, o));
                totals[fi] += 1;
            }
            o += STEP;
        }
    }
    let scan = ScanOptions { overlap: WINDOW, ..*opts };
    let hits: Vec<Vec<(usize, usize)>> = par_chunks(src, &scan, |_, data, own| {
        let mut out = Vec::new();
        let limit = own.min(data.len());
        let mut j = 0usize;
        while j + WINDOW <= data.len() && j < limit {
            let k = key_of(&data[j..j + 8]);
            if bloom[slot(k)]
                && let Some(list) = by_key.get(&k)
            {
                for &(fi, fo) in list {
                    if data[j..j + WINDOW] == files[fi].1[fo..fo + WINDOW] {
                        out.push((fi, fo));
                    }
                }
            }
            j += 1;
        }
        out
    });
    let mut found: Vec<std::collections::HashSet<usize>> = vec![std::collections::HashSet::new(); files.len()];
    for (fi, fo) in hits.into_iter().flatten() {
        found[fi].insert(fo);
    }
    files
        .iter()
        .enumerate()
        .map(|(fi, (name, data))| Coverage {
            name: name.clone(),
            windows: totals[fi],
            found: found[fi].len(),
            whole: blobs.iter().any(|b| b.bytes == *data),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    fn vtf(w: u16, h: u16, seed: u8) -> Vec<u8> {
        let mut d = vec![0u8; 64];
        d[0..4].copy_from_slice(SIGNATURE);
        d[4..8].copy_from_slice(&7u32.to_le_bytes());
        d[8..12].copy_from_slice(&1u32.to_le_bytes());
        d[12..16].copy_from_slice(&64u32.to_le_bytes());
        d[16..18].copy_from_slice(&w.to_le_bytes());
        d[18..20].copy_from_slice(&h.to_le_bytes());
        d[24..26].copy_from_slice(&1u16.to_le_bytes());
        d[52..56].copy_from_slice(&12i32.to_le_bytes());
        d[56] = 1;
        d[57..61].copy_from_slice(&(-1i32).to_le_bytes());
        let mut state = u32::from(seed).wrapping_mul(2_654_435_761) | 1;
        let body: Vec<u8> = (0..usize::from(w) * usize::from(h) * 4)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                (state >> 11) as u8
            })
            .collect();
        d.extend(body);
        d
    }

    #[test]
    fn a_whole_vtf_file_in_memory_is_found_and_counted_as_whole() {
        let file = vtf(16, 16, 29);
        let mut m = VecMem::default();
        m.add(0x1000_0000, [vec![0x55u8; 100], file.clone(), vec![0x66u8; 50]].concat());
        let blobs = find_blobs(&m, &ScanOptions::default());
        assert_eq!(blobs.len(), 1);
        assert_eq!((blobs[0].width, blobs[0].height, blobs[0].format), (16, 16, 12));
        assert_eq!(blobs[0].bytes, file);
        let cov = coverage(&m, &ScanOptions::default(), &[("a".to_owned(), file)], &blobs);
        assert!(cov[0].whole);
        assert!(cov[0].windows > 0);
        assert_eq!(cov[0].found, cov[0].windows);
    }

    #[test]
    fn a_texture_that_is_nowhere_in_memory_has_zero_coverage() {
        let file = vtf(16, 16, 29);
        let mut m = VecMem::default();
        m.add(0x1000_0000, vec![0x11u8; 4096]);
        let blobs = find_blobs(&m, &ScanOptions::default());
        assert!(blobs.is_empty());
        let cov = coverage(&m, &ScanOptions::default(), &[("a".to_owned(), file)], &blobs);
        assert_eq!((cov[0].found, cov[0].whole), (0, false));
        assert_eq!(cov[0].fraction(), 0.0);
    }

    #[test]
    fn a_truncated_copy_is_partial_and_not_a_blob() {
        let file = vtf(32, 32, 7);
        let mut m = VecMem::default();
        m.add(0x1000_0000, file[..file.len() / 2].to_vec());
        let blobs = find_blobs(&m, &ScanOptions::default());
        assert!(blobs.is_empty());
        let cov = coverage(&m, &ScanOptions::default(), &[("a".to_owned(), file)], &blobs);
        assert!(cov[0].found > 0 && cov[0].found < cov[0].windows, "found {} of {}", cov[0].found, cov[0].windows);
        assert!(!cov[0].whole);
    }
}
