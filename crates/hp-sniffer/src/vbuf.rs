use ahash::AHashMap;
use rayon::prelude::*;

use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

pub const VERTEX_STRIDE: usize = 48;
const NORMAL_AT: usize = 12;
const COLOR_AT: usize = 24;
const BLOOM_BITS: usize = 1 << 20;
const POSITION_TOLERANCE: f32 = 0.01;
const QUANT: f32 = 4.0;
const NORMAL_MIN: f32 = 0.5;
const NORMAL_MAX: f32 = 1.5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlphaFound {
    pub addr: u64,
    pub alphas: Vec<u8>,
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn quant(v: f32) -> i32 {
    (v * QUANT).round() as i32
}

fn bloom_slot(q: i32) -> usize {
    let x = q as u32;
    ((x ^ (x >> 15)).wrapping_mul(0x9e37_79b1) >> 12) as usize & (BLOOM_BITS - 1)
}

fn key_of(p: &[f32; 3]) -> [i32; 3] {
    [quant(p[0]), quant(p[1]), quant(p[2])]
}

pub fn match_array(data: &[u8], positions: &[[f32; 3]]) -> Option<Vec<u8>> {
    if data.len() < positions.len() * VERTEX_STRIDE {
        return None;
    }
    let mut alphas = Vec::with_capacity(positions.len());
    for (k, p) in positions.iter().enumerate() {
        let at = k * VERTEX_STRIDE;
        if (0..3).any(|c| (f32_at(data, at + c * 4) - p[c]).abs() > POSITION_TOLERANCE) {
            return None;
        }
        let n = [f32_at(data, at + NORMAL_AT), f32_at(data, at + NORMAL_AT + 4), f32_at(data, at + NORMAL_AT + 8)];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        if !len.is_finite() || !(NORMAL_MIN..=NORMAL_MAX).contains(&len) {
            return None;
        }
        alphas.push(data[at + COLOR_AT + 3]);
    }
    Some(alphas)
}

pub fn find_vertex_alphas<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    targets: &[&[[f32; 3]]],
) -> Vec<Option<AlphaFound>> {
    let mut by_first: AHashMap<[i32; 3], Vec<usize>> = AHashMap::new();
    let mut bloom = vec![0u64; BLOOM_BITS / 64];
    let mut longest = 0usize;
    for (i, t) in targets.iter().enumerate() {
        let Some(first) = t.first() else { continue };
        by_first.entry(key_of(first)).or_default().push(i);
        let slot = bloom_slot(quant(first[0]));
        bloom[slot / 64] |= 1 << (slot % 64);
        longest = longest.max(t.len());
    }
    if by_first.is_empty() {
        return vec![None; targets.len()];
    }
    let opts = ScanOptions { overlap: longest * VERTEX_STRIDE + 16, ..*opts };
    let hits: Vec<Vec<(u64, usize)>> = par_chunks(src, &opts, |base, data, own| {
        let mut out = Vec::new();
        let limit = own.min(data.len());
        let mut j = (4 - (base % 4) as usize) % 4;
        while j + 12 <= data.len() && j < limit {
            let x = f32_at(data, j);
            if !x.is_finite() || x.abs() > 1.0e6 {
                j += 4;
                continue;
            }
            let slot = bloom_slot(quant(x));
            if bloom[slot / 64] & (1 << (slot % 64)) != 0
                && let Some(list) = by_first.get(&[quant(x), quant(f32_at(data, j + 4)), quant(f32_at(data, j + 8))])
            {
                for &t in list {
                    let need = targets[t].len() * VERTEX_STRIDE;
                    if let Some(window) = data.get(j..j + need)
                        && match_array(window, targets[t]).is_some()
                    {
                        out.push((base + j as u64, t));
                    }
                }
            }
            j += 4;
        }
        out
    });
    let mut all: Vec<(u64, usize)> = hits.into_iter().flatten().collect();
    all.sort_unstable();
    all.dedup();
    let reads: Vec<(usize, u64, Option<Vec<u8>>)> = all
        .par_iter()
        .map(|&(addr, t)| {
            let need = targets[t].len() * VERTEX_STRIDE;
            let raw = src.read_vec(addr, need);
            (t, addr, match_array(&raw, targets[t]))
        })
        .collect();
    let mut out: Vec<Option<AlphaFound>> = vec![None; targets.len()];
    for (t, addr, alphas) in reads {
        let Some(alphas) = alphas else { continue };
        let better = match &out[t] {
            None => true,
            Some(old) => variety(&alphas) > variety(&old.alphas),
        };
        if better {
            out[t] = Some(AlphaFound { addr, alphas });
        }
    }
    out
}

fn variety(a: &[u8]) -> usize {
    let mut seen = [false; 256];
    for &v in a {
        seen[v as usize] = true;
    }
    seen.iter().filter(|s| **s).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    fn record(p: [f32; 3], alpha: u8) -> Vec<u8> {
        let mut r = vec![0u8; VERTEX_STRIDE];
        for (k, c) in p.iter().enumerate() {
            r[k * 4..k * 4 + 4].copy_from_slice(&c.to_le_bytes());
        }
        r[NORMAL_AT..NORMAL_AT + 4].copy_from_slice(&0f32.to_le_bytes());
        r[NORMAL_AT + 4..NORMAL_AT + 8].copy_from_slice(&0f32.to_le_bytes());
        r[NORMAL_AT + 8..NORMAL_AT + 12].copy_from_slice(&1f32.to_le_bytes());
        r[COLOR_AT..COLOR_AT + 4].copy_from_slice(&[255, 255, 255, alpha]);
        r
    }

    fn grid(base: f32) -> Vec<[f32; 3]> {
        (0..25).map(|k| [base + (k % 5) as f32 * 8.0, (k / 5) as f32 * 8.0 + 0.25, 3.5 + k as f32]).collect()
    }

    #[test]
    fn alphas_are_read_from_the_vertex_records_in_order() {
        let a = grid(0.0);
        let b = grid(500.0);
        let mut buf = vec![0xabu8; 40];
        for (k, p) in a.iter().enumerate() {
            buf.extend(record(*p, (k * 10) as u8));
        }
        buf.extend(vec![0u8; 20]);
        for (k, p) in b.iter().enumerate() {
            buf.extend(record(*p, 255 - k as u8));
        }
        let mut mem = VecMem::default();
        mem.add(0x4000_0000, buf);
        let found = find_vertex_alphas(&mem, &ScanOptions::default(), &[&a, &b, &grid(900.0)]);
        let fa = found[0].as_ref().expect("first");
        assert_eq!(fa.addr, 0x4000_0000 + 40);
        assert_eq!(fa.alphas, (0..25).map(|k| (k * 10) as u8).collect::<Vec<_>>());
        assert_eq!(found[1].as_ref().expect("second").alphas[24], 231);
        assert!(found[2].is_none(), "an array that is not in memory is not found");
    }

    #[test]
    fn a_copy_with_a_bad_normal_is_refused() {
        let a = grid(0.0);
        let mut buf = Vec::new();
        for p in &a {
            let mut r = record(*p, 9);
            r[NORMAL_AT + 8..NORMAL_AT + 12].copy_from_slice(&5f32.to_le_bytes());
            buf.extend(r);
        }
        let mut mem = VecMem::default();
        mem.add(0x4000_0000, buf);
        assert!(find_vertex_alphas(&mem, &ScanOptions::default(), &[&a])[0].is_none());
    }

    #[test]
    fn the_copy_with_the_most_distinct_alphas_wins() {
        let a = grid(0.0);
        let mut buf = Vec::new();
        for p in &a {
            buf.extend(record(*p, 255));
        }
        buf.extend(vec![0u8; 64]);
        for (k, p) in a.iter().enumerate() {
            buf.extend(record(*p, (k * 3) as u8));
        }
        let mut mem = VecMem::default();
        mem.add(0x4000_0000, buf);
        let f = find_vertex_alphas(&mem, &ScanOptions::default(), &[&a]);
        assert_eq!(f[0].as_ref().expect("found").alphas[7], 21);
    }
}
