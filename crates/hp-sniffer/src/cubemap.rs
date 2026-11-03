use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

pub const SAMPLE_RECORD: usize = 32;
const POINTER_AT: usize = 16;
const SIZE_AT: usize = 24;
const PAD_AT: usize = 12;
const MAX_SIZE: i32 = 1024;
const MAX_COORD: f32 = 131_072.0;

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn u64_at(d: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
}

pub fn parse_sample(rec: &[u8]) -> Option<[i32; 4]> {
    if rec.len() < SAMPLE_RECORD {
        return None;
    }
    let mut out = [0i32; 4];
    for (k, slot) in out.iter_mut().take(3).enumerate() {
        let v = f32_at(rec, k * 4);
        if !v.is_finite() || v.abs() > MAX_COORD || v != v.round() {
            return None;
        }
        *slot = v as i32;
    }
    let ptr = u64_at(rec, POINTER_AT);
    let size = i32_at(rec, SIZE_AT);
    if i32_at(rec, PAD_AT) != 0 || !(0x1_0000..0x7fff_ffff_ffff).contains(&ptr) || !(0..=MAX_SIZE).contains(&size) {
        return None;
    }
    out[3] = size;
    Some(out)
}

pub fn sample_runs(data: &[u8]) -> Vec<(usize, Vec<[i32; 4]>)> {
    let mut runs = Vec::new();
    let mut j = 0usize;
    while j + SAMPLE_RECORD <= data.len() {
        if parse_sample(&data[j..]).is_none() {
            j += 4;
            continue;
        }
        let mut list = Vec::new();
        let mut at = j;
        while at + SAMPLE_RECORD <= data.len() {
            match parse_sample(&data[at..]) {
                Some(s) => {
                    list.push(s);
                    at += SAMPLE_RECORD;
                }
                None => break,
            }
        }
        runs.push((j, list));
        j = at;
    }
    runs
}

pub fn find_cubemap_samples<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    names: &[[i32; 3]],
) -> Option<Vec<[i32; 4]>> {
    if names.is_empty() {
        return None;
    }
    let opts = ScanOptions { overlap: 1024 * SAMPLE_RECORD, writable_only: true, ..*opts };
    let runs: Vec<Vec<[i32; 4]>> = par_chunks(src, &opts, |_, data, own| {
        sample_runs(data).into_iter().filter(|(at, _)| *at < own).map(|(_, l)| l).collect::<Vec<_>>()
    })
    .into_iter()
    .flatten()
    .collect();
    let known: std::collections::HashSet<[i32; 3]> = names.iter().copied().collect();
    let score = |r: &Vec<[i32; 4]>| r.iter().filter(|s| known.contains(&[s[0], s[1], s[2]])).count();
    let best = runs.iter().max_by_key(|r| (score(r), r.len()))?;
    let covered: std::collections::HashSet<[i32; 3]> =
        best.iter().map(|s| [s[0], s[1], s[2]]).filter(|o| known.contains(o)).collect();
    if covered.len() != known.len() {
        return None;
    }
    let is_known = |s: &&[i32; 4]| known.contains(&[s[0], s[1], s[2]]);
    let first = best.iter().position(|s| is_known(&s))?;
    let last = best.iter().rposition(|s| is_known(&s))?;
    Some(best[first..=last].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    pub(crate) fn sample_bytes(s: [i32; 4], ptr: u64) -> Vec<u8> {
        let mut r = vec![0u8; SAMPLE_RECORD];
        for k in 0..3 {
            r[k * 4..k * 4 + 4].copy_from_slice(&(s[k] as f32).to_le_bytes());
        }
        r[POINTER_AT..POINTER_AT + 8].copy_from_slice(&ptr.to_le_bytes());
        r[SIZE_AT..SIZE_AT + 4].copy_from_slice(&s[3].to_le_bytes());
        r
    }

    #[test]
    fn samples_are_read_with_their_sizes_in_order() {
        let list = [[1280, 2432, 32, 8], [1344, 3584, 32, 1], [832, 384, -96, 9], [832, 384, -96, 0]];
        let mut data = vec![0x77u8; 16];
        for s in list {
            data.extend(sample_bytes(s, 0x042f_be60));
        }
        data.extend(vec![0u8; 64]);
        let runs = sample_runs(&data);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].1, list.to_vec());
    }

    #[test]
    fn the_run_that_holds_every_named_origin_is_taken() {
        let list = [[0, 0, 64, 5], [128, 0, 64, 6], [0, 128, 64, 7]];
        let mut buf = Vec::new();
        for s in list {
            buf.extend(sample_bytes(s, 0x1000_0000));
        }
        let mut mem = VecMem::default();
        mem.add(0x5000_0000, buf);
        let names = [[0, 0, 64], [128, 0, 64], [0, 128, 64]];
        let got = find_cubemap_samples(&mem, &ScanOptions::default(), &names).expect("samples");
        assert_eq!(got, list.to_vec());
        let missing = [[0, 0, 64], [999, 0, 64]];
        assert!(find_cubemap_samples(&mem, &ScanOptions::default(), &missing).is_none());
    }

    #[test]
    fn a_record_with_a_fractional_origin_or_a_null_pointer_is_not_a_sample() {
        let mut r = sample_bytes([1, 2, 3, 4], 0x1000_0000);
        assert!(parse_sample(&r).is_some());
        r[0..4].copy_from_slice(&1.5f32.to_le_bytes());
        assert!(parse_sample(&r).is_none());
        assert!(parse_sample(&sample_bytes([1, 2, 3, 4], 0)).is_none());
        assert!(parse_sample(&sample_bytes([1, 2, 3, 5000], 0x1000_0000)).is_none());
    }
}
