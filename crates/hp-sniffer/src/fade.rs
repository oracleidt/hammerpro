use rayon::prelude::*;

use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

pub const FADE_RECORD: usize = 16;
const ALPHA_RANGE: f32 = 255.0;
const MAX_SQUARE: f32 = 1.0e11;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FadeEntry {
    pub index: u32,
    pub min_sq: f32,
    pub max_sq: f32,
    pub squared: bool,
}

impl FadeEntry {
    pub fn min(&self) -> f32 {
        if self.squared { distance_from_square(self.min_sq) } else { self.min_sq }
    }

    pub fn max(&self) -> f32 {
        if self.squared { distance_from_square(self.max_sq) } else { self.max_sq }
    }
}

pub fn distance_from_square(sq: f32) -> f32 {
    if sq <= 0.0 {
        return 0.0;
    }
    let root = f64::from(sq).sqrt();
    let whole = root.round();
    if (whole as f32) * (whole as f32) == sq {
        return whole as f32;
    }
    let guess = root as f32;
    for cand in [guess, f32::from_bits(guess.to_bits() + 1), f32::from_bits(guess.to_bits().saturating_sub(1))] {
        if cand * cand == sq {
            return cand;
        }
    }
    guess
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

pub fn parse_fade_record(rec: &[u8]) -> Option<FadeEntry> {
    if rec.len() < FADE_RECORD {
        return None;
    }
    let index = u32::from_le_bytes([rec[0], rec[1], rec[2], rec[3]]);
    let (min_sq, max_sq, scale) = (f32_at(rec, 4), f32_at(rec, 8), f32_at(rec, 12));
    if index > 1_000_000 || !min_sq.is_finite() || !max_sq.is_finite() || !scale.is_finite() || min_sq < 0.0 {
        return None;
    }
    if max_sq > min_sq && max_sq <= MAX_SQUARE {
        let want = ALPHA_RANGE / (max_sq - min_sq);
        if (scale - want).abs() <= want * 2.0e-3 {
            return Some(FadeEntry { index, min_sq, max_sq, squared: true });
        }
    }
    if max_sq != min_sq && max_sq > 0.0 && max_sq <= MAX_SQUARE {
        let want = ALPHA_RANGE / (min_sq - max_sq);
        if (scale - want).abs() <= want.abs() * 2.0e-3 {
            return Some(FadeEntry { index, min_sq, max_sq, squared: false });
        }
    }
    None
}

pub fn fade_runs(data: &[u8]) -> Vec<(usize, Vec<FadeEntry>)> {
    let mut runs = Vec::new();
    let mut j = 0usize;
    while j + FADE_RECORD <= data.len() {
        let Some(first) = parse_fade_record(&data[j..]) else {
            j += 4;
            continue;
        };
        let mut list = vec![first];
        let mut at = j + FADE_RECORD;
        while at + FADE_RECORD <= data.len() {
            match parse_fade_record(&data[at..]) {
                Some(next) if next.index > list.last().expect("not empty").index => {
                    list.push(next);
                    at += FADE_RECORD;
                }
                _ => break,
            }
        }
        runs.push((j, list));
        j = at;
    }
    runs
}

pub fn find_fade_runs<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> Vec<(u64, Vec<FadeEntry>)> {
    let opts = ScanOptions { overlap: 4096 * FADE_RECORD, writable_only: true, ..*opts };
    let mut out: Vec<(u64, Vec<FadeEntry>)> = par_chunks(src, &opts, |base, data, own| {
        fade_runs(data)
            .into_iter()
            .filter(|(at, _)| *at < own)
            .map(|(at, list)| (base + at as u64, list))
            .collect::<Vec<_>>()
    })
    .into_par_iter()
    .flatten()
    .collect();
    out.sort_by_key(|r| r.0);
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn fade_bytes(entries: &[(u32, f32, f32)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (i, mn, mx) in entries {
            out.extend_from_slice(&i.to_le_bytes());
            out.extend_from_slice(&(mn * mn).to_le_bytes());
            out.extend_from_slice(&(mx * mx).to_le_bytes());
            out.extend_from_slice(&(ALPHA_RANGE / (mx * mx - mn * mn)).to_le_bytes());
        }
        out
    }

    #[test]
    fn squares_are_turned_back_into_distances() {
        assert_eq!(distance_from_square(202_500.0), 450.0);
        assert_eq!(distance_from_square(0.0), 0.0);
        let d = 1234.567f32;
        assert_eq!(distance_from_square(d * d), d);
        let d = 0.1f32;
        assert_eq!(distance_from_square(d * d), d);
    }

    #[test]
    fn a_table_of_fade_records_is_read() {
        let mut data = vec![0x11u8; 24];
        data.extend(fade_bytes(&[(0, 300.0, 400.0), (2, 450.0, 1250.0), (3, 0.0, 3500.0), (9, 200.0, 300.0)]));
        data.extend(vec![0u8; 32]);
        let runs = fade_runs(&data);
        assert_eq!(runs.len(), 1);
        let (at, list) = &runs[0];
        assert_eq!(*at, 24);
        assert_eq!(list.len(), 4);
        assert_eq!((list[1].index, list[1].min(), list[1].max()), (2, 450.0, 1250.0));
        assert_eq!((list[2].min(), list[2].max()), (0.0, 3500.0));
    }

    #[test]
    fn screen_space_records_keep_their_raw_values() {
        let mut rec = Vec::new();
        rec.extend_from_slice(&7u32.to_le_bytes());
        rec.extend_from_slice(&200.0f32.to_le_bytes());
        rec.extend_from_slice(&1700.0f32.to_le_bytes());
        rec.extend_from_slice(&(ALPHA_RANGE / (200.0 - 1700.0)).to_le_bytes());
        let e = parse_fade_record(&rec).expect("record");
        assert!(!e.squared);
        assert_eq!((e.index, e.min(), e.max()), (7, 200.0, 1700.0));
        let mut mixed = fade_bytes(&[(5, 100.0, 200.0)]);
        mixed.extend_from_slice(&rec);
        assert_eq!(fade_runs(&mixed)[0].1.len(), 2);
    }

    #[test]
    fn records_with_a_wrong_scale_or_order_are_not_a_table() {
        let mut bad = fade_bytes(&[(1, 100.0, 200.0)]);
        bad[12..16].copy_from_slice(&1.0f32.to_le_bytes());
        assert!(parse_fade_record(&bad).is_none());
        let backwards = fade_bytes(&[(5, 100.0, 200.0), (4, 100.0, 200.0)]);
        let runs = fade_runs(&backwards);
        assert_eq!(runs.iter().map(|r| r.1.len()).collect::<Vec<_>>(), vec![1, 1]);
        assert!(parse_fade_record(&fade_bytes(&[(1, 300.0, 200.0)])).is_none());
    }
}
