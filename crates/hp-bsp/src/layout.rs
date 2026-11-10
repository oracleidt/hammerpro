use std::borrow::Cow;

use crate::header::{HEADER_LEN, Header, LUMP_COUNT, LumpInfo, lump};
use crate::rng::Rng;
use crate::{Bsp, BspError, Result};

const VERSION_FREE_LUMPS: &[usize] = &[0, 1, 2, 3, 4, 5, 6, 12, 13, 14, 16, 17, 18, 19, 20, 21, 40, 43, 44];
const STALE_COPY_MAX: usize = 1_500_000;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutOptions {
    pub shuffle_lumps: bool,
    pub junk_gaps: bool,
    pub decoys: bool,
    pub scramble_header: bool,
    pub shuffle_game_lumps: bool,
    pub random_pak: bool,
}

impl LayoutOptions {
    pub fn strongest() -> LayoutOptions {
        LayoutOptions {
            shuffle_lumps: true,
            junk_gaps: true,
            decoys: true,
            scramble_header: true,
            shuffle_game_lumps: true,
            random_pak: true,
        }
    }

    pub fn any(&self) -> bool {
        self.shuffle_lumps
            || self.junk_gaps
            || self.decoys
            || self.scramble_header
            || self.shuffle_game_lumps
            || self.random_pak
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutReport {
    pub decoy_blocks: usize,
    pub decoy_bytes: usize,
    pub gap_bytes: usize,
    pub pak_index: Option<usize>,
}

enum Item {
    Lump(usize),
    Decoy(Vec<u8>),
}

fn align4(n: usize) -> usize {
    (n + 3) & !3
}

fn mutate(data: &mut [u8], text: bool, rng: &mut Rng) {
    if data.is_empty() {
        return;
    }
    let hits = (data.len() / 200).max(2);
    for _ in 0..hits {
        let at = rng.below(data.len());
        if text {
            if data[at].is_ascii_digit() {
                data[at] = b'0' + rng.below(10) as u8;
            }
        } else {
            data[at] ^= (rng.next_u64() as u8) | 1;
        }
    }
}

fn make_decoy(bsp: &Bsp, present: &[usize], budget: usize, rng: &mut Rng) -> Vec<u8> {
    let candidates: Vec<usize> = present
        .iter()
        .copied()
        .filter(|&i| i != lump::PAKFILE && i != lump::GAME_LUMP && !bsp.is_compressed(i))
        .filter(|&i| {
            let n = bsp.header().lumps[i].len as usize;
            n >= 16 && n <= STALE_COPY_MAX.min(budget)
        })
        .collect();
    let kind = rng.below(4);
    if kind >= 2 && !candidates.is_empty() {
        let i = candidates[rng.below(candidates.len())];
        let mut data = bsp.lump_raw(i).map(<[u8]>::to_vec).unwrap_or_default();
        mutate(&mut data, i == lump::ENTITIES, rng);
        if kind == 3 {
            let keep = data.len() * (60 + rng.below(36)) / 100;
            data.truncate(keep.max(16));
        }
        return data;
    }
    let size = (256 + rng.below(16_000)).min(budget.max(16));
    rng.bytes(size)
}

pub fn relayout(bytes: &[u8], opts: &LayoutOptions, rng: &mut Rng) -> Result<(Vec<u8>, LayoutReport)> {
    let mut report = LayoutReport::default();
    if !opts.any() {
        return Ok((bytes.to_vec(), report));
    }
    let bsp = Bsp::from_vec(bytes.to_vec())?;
    let src = bsp.header();
    let present: Vec<usize> = (0..LUMP_COUNT).filter(|&i| src.lumps[i].len > 0).collect();
    let mut ids: Vec<usize> = present.iter().copied().filter(|&i| i != lump::PAKFILE).collect();
    ids.sort_by_key(|&i| (src.lumps[i].offset, i));
    if opts.shuffle_lumps {
        rng.shuffle(&mut ids);
    }
    let mut items: Vec<Item> = ids.into_iter().map(Item::Lump).collect();
    let has_pak = present.contains(&lump::PAKFILE);
    let mut pak_last = true;
    if has_pak {
        if opts.random_pak {
            let at = rng.below(items.len() + 1);
            pak_last = at == items.len();
            items.insert(at, Item::Lump(lump::PAKFILE));
        } else {
            items.push(Item::Lump(lump::PAKFILE));
        }
    }
    if opts.decoys {
        let mut budget = (bytes.len() / 8).clamp(64 * 1024, 6 * 1024 * 1024);
        let count = 4 + rng.below(9);
        for _ in 0..count {
            if budget < 64 {
                break;
            }
            let data = make_decoy(&bsp, &present, budget, rng);
            budget = budget.saturating_sub(data.len());
            report.decoy_bytes += data.len();
            report.decoy_blocks += 1;
            let limit = if has_pak && pak_last { items.len() - 1 } else { items.len() };
            let at = rng.below(limit + 1);
            items.insert(at, Item::Decoy(data));
        }
    }

    let mut game = if present.contains(&lump::GAME_LUMP) { Some(bsp.game_lumps()?) } else { None };
    if opts.shuffle_game_lumps
        && let Some(g) = game.as_mut()
    {
        rng.shuffle(&mut g.lumps);
    }

    let mut out: Vec<u8> = vec![0u8; HEADER_LEN];
    let mut lumps = [LumpInfo::default(); LUMP_COUNT];
    let mut gap_total = 0usize;
    for (n, item) in items.iter().enumerate() {
        let before = out.len();
        let mut gap = 0usize;
        if opts.junk_gaps && n > 0 {
            gap = match rng.below(10) {
                0..=4 => 4 * rng.below(4),
                5..=8 => 4 * rng.below(32),
                _ => 4 * rng.below(256),
            };
        }
        let junk = rng.bytes(gap);
        out.extend_from_slice(&junk);
        gap_total += out.len() - before;
        match item {
            Item::Decoy(data) => {
                out.extend_from_slice(data);
            }
            Item::Lump(i) => {
                let info = src.lumps[*i];
                let offset = out.len();
                let too_big = || BspError::TooLarge("bsp".into());
                let (data, four_cc): (Cow<'_, [u8]>, [u8; 4]) = if *i == lump::GAME_LUMP {
                    let g = game.as_ref().ok_or_else(|| BspError::corrupt("game lump vanished"))?;
                    let ofs = u32::try_from(offset).map_err(|_| too_big())?;
                    (Cow::Owned(g.to_bytes(ofs)?), [0; 4])
                } else {
                    (Cow::Borrowed(bsp.lump_raw(*i)?), info.four_cc)
                };
                lumps[*i] = LumpInfo {
                    offset: u32::try_from(offset).map_err(|_| too_big())?,
                    len: data.len() as u32,
                    version: info.version,
                    four_cc,
                };
                out.extend_from_slice(&data);
            }
        }
        let aligned = align4(out.len());
        let pad = aligned - out.len();
        if opts.junk_gaps {
            let junk = rng.bytes(pad);
            out.extend_from_slice(&junk);
        } else {
            out.resize(aligned, 0);
        }
    }
    report.gap_bytes = gap_total;
    report.pak_index = items.iter().position(|it| matches!(it, Item::Lump(i) if *i == lump::PAKFILE));
    if out.len() > i32::MAX as usize {
        return Err(BspError::TooLarge("bsp".into()));
    }

    let mut map_revision = src.map_revision;
    if opts.scramble_header {
        map_revision = (rng.next_u64() % 0x7fff_fff0) as i32 + 1;
        let slots = ((out.len() - HEADER_LEN) / 4).max(1);
        for (i, info) in lumps.iter_mut().enumerate() {
            if info.len == 0 {
                let o = (HEADER_LEN + 4 * rng.below(slots)) as u32;
                *info = LumpInfo { offset: o, len: 0, version: 0, four_cc: [0; 4] };
            } else if VERSION_FREE_LUMPS.contains(&i) {
                info.version = (rng.next_u64() & 7) as u32;
            }
        }
    }
    let header = Header { version: src.version, lumps, map_revision, layout: src.layout };
    out[..HEADER_LEN].copy_from_slice(&header.to_bytes());

    let check = Bsp::from_vec(out.clone())?;
    for &i in &present {
        let same = if i == lump::GAME_LUMP {
            let a = bsp.game_lumps()?;
            let b = check.game_lumps()?;
            a.lumps.len() == b.lumps.len() && a.lumps.iter().all(|l| b.get(l.id).is_some_and(|m| m.stored == l.stored))
        } else {
            bsp.lump_raw(i)? == check.lump_raw(i)?
        };
        if !same {
            return Err(BspError::corrupt(format!("the shuffled layout changed lump {i}")));
        }
    }
    Ok((out, report))
}
