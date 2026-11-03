use ahash::AHashMap;
use hp_bsp::Bsp;
use memchr::memmem;
use rayon::prelude::*;

use crate::procmem::{MemSource, Region};

pub const WORLDSPAWN_NEEDLE: &[u8] = b"\"classname\" \"worldspawn\"";
pub const MAX_ENTITY_TEXT: usize = 256 << 20;
pub const SAMPLES_PER_LUMP: usize = 16;
const PLANE_RECORD: usize = 20;
const PLANE_LUMP: usize = 1;

#[derive(Clone, Copy, Debug)]
pub struct ScanOptions {
    pub chunk: usize,
    pub overlap: usize,
    pub writable_only: bool,
    pub min_region: u64,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self { chunk: 8 << 20, overlap: 64, writable_only: false, min_region: 0 }
    }
}

pub fn merged_spans(regions: &[Region], opts: &ScanOptions) -> Vec<(u64, u64)> {
    let mut sorted: Vec<&Region> = regions
        .iter()
        .filter(|r| !r.image && r.size >= opts.min_region.min(r.size) && (!opts.writable_only || r.writable))
        .collect();
    sorted.sort_by_key(|r| r.base);
    let mut spans: Vec<(u64, u64)> = Vec::new();
    for r in sorted {
        match spans.last_mut() {
            Some(last) if last.0 + last.1 == r.base => last.1 += r.size,
            _ => spans.push((r.base, r.size)),
        }
    }
    spans.retain(|&(_, size)| size >= opts.min_region);
    spans
}

pub fn plan_chunks(regions: &[Region], opts: &ScanOptions) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    for (base, size) in merged_spans(regions, opts) {
        let mut off = 0u64;
        while off < size {
            let n = (opts.chunk as u64 + opts.overlap as u64).min(size - off);
            out.push((base + off, n));
            if off + n >= size {
                break;
            }
            off += opts.chunk as u64;
        }
    }
    out
}

pub fn par_chunks<S, R, F>(src: &S, opts: &ScanOptions, f: F) -> Vec<R>
where
    S: MemSource + ?Sized,
    R: Send,
    F: Fn(u64, &[u8], usize) -> R + Sync,
{
    let plan = plan_chunks(&src.regions(), opts);
    plan.par_iter()
        .map(|&(base, len)| {
            let mut buf = vec![0u8; len as usize];
            let got = src.read(base, &mut buf);
            buf.truncate(got);
            let own = opts.chunk.min(buf.len());
            f(base, &buf, own)
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityHit {
    pub addr: u64,
    pub text: String,
    pub entities: usize,
}

fn is_text_byte(b: u8) -> bool {
    b >= 0x80 || b == b'\n' || b == b'\r' || b == b'\t' || (0x20..0x7f).contains(&b)
}

pub fn extract_entity_text<S: MemSource + ?Sized>(src: &S, hit: u64) -> Option<(u64, usize, String)> {
    let mut start = hit;
    let mut guard = 0usize;
    'back: loop {
        if start == 0 {
            break;
        }
        let from = (start - 1) & !0xfff;
        let len = (start - from) as usize;
        let window = src.read_vec(from, len);
        if window.len() != len {
            break;
        }
        for (i, b) in window.iter().enumerate().rev() {
            if !is_text_byte(*b) {
                start = from + i as u64 + 1;
                break 'back;
            }
        }
        start = from;
        guard += len;
        if guard > MAX_ENTITY_TEXT {
            return None;
        }
    }
    let mut data = Vec::new();
    let mut at = start;
    let step = 1usize << 20;
    loop {
        let chunk = src.read_vec(at, step);
        if chunk.is_empty() {
            break;
        }
        match chunk.iter().position(|b| !is_text_byte(*b)) {
            Some(end) => {
                data.extend_from_slice(&chunk[..end]);
                break;
            }
            None => {
                data.extend_from_slice(&chunk);
                at += chunk.len() as u64;
            }
        }
        if data.len() > MAX_ENTITY_TEXT {
            return None;
        }
    }
    let raw_len = data.len();
    let text = String::from_utf8_lossy(&data).into_owned();
    let trimmed = text.trim_start();
    let skipped = (text.len() - trimmed.len()) as u64;
    if !trimmed.starts_with('{') {
        let cut = trimmed.find("{\n")?;
        return Some((start + skipped + cut as u64, raw_len, trimmed[cut..].to_owned()));
    }
    Some((start + skipped, raw_len, trimmed.to_owned()))
}

pub fn find_entity_lumps<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> Vec<EntityHit> {
    let finder = memmem::Finder::new(WORLDSPAWN_NEEDLE);
    let mut addrs: Vec<u64> = par_chunks(src, opts, |base, data, own| {
        finder.find_iter(data).filter(|&j| j < own || own == data.len()).map(|j| base + j as u64).collect::<Vec<u64>>()
    })
    .into_iter()
    .flatten()
    .collect();
    addrs.sort_unstable();
    addrs.dedup();
    let mut covered: Vec<(u64, u64)> = Vec::new();
    let mut out = Vec::new();
    for addr in addrs {
        if covered.iter().any(|&(lo, hi)| addr >= lo && addr < hi) {
            continue;
        }
        let Some((start, raw_len, text)) = extract_entity_text(src, addr) else { continue };
        covered.push((start, start + raw_len as u64 + 1));
        let entities = hp_bsp::parse_entities(&text);
        if entities.len() < 2 || !entities.iter().any(|e| e.classname().eq_ignore_ascii_case("worldspawn")) {
            continue;
        }
        out.push(EntityHit { addr: start, entities: entities.len(), text });
    }
    out.sort_by(|a, b| b.entities.cmp(&a.entities).then(a.addr.cmp(&b.addr)));
    out.dedup_by(|a, b| a.text == b.text);
    out
}

#[derive(Clone, Debug)]
pub struct LumpProbe {
    pub lump: usize,
    pub len: usize,
    pub samples: Vec<(usize, u128)>,
    pub plane_like: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coverage {
    Verified,
    Mostly,
    Partial,
    Absent,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LumpCoverage {
    pub lump: usize,
    pub len: usize,
    pub samples_total: usize,
    pub samples_hit: usize,
    pub base: Option<u64>,
    pub match_percent: f64,
    pub state: Coverage,
}

fn key_at(data: &[u8], at: usize) -> Option<u128> {
    let b: [u8; 16] = data.get(at..at + 16)?.try_into().ok()?;
    Some(u128::from_le_bytes(b))
}

fn good_key(key: u128) -> bool {
    let bytes = key.to_le_bytes();
    let mut seen = [false; 256];
    let mut distinct = 0;
    for b in bytes {
        if !seen[b as usize] {
            seen[b as usize] = true;
            distinct += 1;
        }
    }
    distinct >= 6
}

pub fn probe_lump(lump: usize, data: &[u8], samples: usize) -> LumpProbe {
    let plane_like = lump == PLANE_LUMP;
    let align = if plane_like { PLANE_RECORD } else { 4 };
    let mut out = Vec::new();
    if data.len() >= 32 {
        let span = data.len() - 16;
        for k in 0..samples {
            let target = span * k / samples.max(1);
            let mut at = target / align * align;
            let mut tries = 0;
            while tries < 64 {
                if let Some(key) = key_at(data, at)
                    && good_key(key)
                    && !out.iter().any(|(_, k2)| *k2 == key)
                {
                    out.push((at, key));
                    break;
                }
                at += align;
                tries += 1;
            }
        }
    }
    LumpProbe { lump, len: data.len(), samples: out, plane_like }
}

pub fn probes_from_bsp(bsp: &Bsp, skip: &[usize]) -> Vec<LumpProbe> {
    let mut out = Vec::new();
    for i in 0..hp_bsp::LUMP_COUNT {
        if skip.contains(&i) {
            continue;
        }
        let Ok(info) = bsp.lump_info(i) else { continue };
        if info.len < 32 || bsp.is_compressed(i) {
            continue;
        }
        let Ok(data) = bsp.lump(i) else { continue };
        let p = probe_lump(i, &data, SAMPLES_PER_LUMP);
        if !p.samples.is_empty() {
            out.push(p);
        }
    }
    out
}

fn compare_percent(mem: &[u8], file: &[u8], plane_like: bool) -> f64 {
    if file.is_empty() {
        return 0.0;
    }
    if plane_like {
        let mut same = 0usize;
        let mut total = 0usize;
        for (m, f) in mem.chunks(PLANE_RECORD).zip(file.chunks(PLANE_RECORD)) {
            if f.len() < 16 {
                break;
            }
            total += 16;
            if m.len() >= 16 {
                same += m[..16].iter().zip(&f[..16]).filter(|(a, b)| a == b).count();
            }
        }
        return if total == 0 { 0.0 } else { same as f64 * 100.0 / total as f64 };
    }
    let same = mem.iter().zip(file).filter(|(a, b)| a == b).count();
    same as f64 * 100.0 / file.len() as f64
}

pub fn scan_probes<S: MemSource + ?Sized>(
    src: &S,
    probes: &[LumpProbe],
    file_lump: &(dyn Fn(usize) -> Option<Vec<u8>> + Sync),
    opts: &ScanOptions,
) -> Vec<LumpCoverage> {
    let mut table: AHashMap<u128, Vec<(usize, usize)>> = AHashMap::new();
    for (pi, p) in probes.iter().enumerate() {
        for (si, (_, key)) in p.samples.iter().enumerate() {
            table.entry(*key).or_default().push((pi, si));
        }
    }
    let hits: Vec<Vec<(usize, usize, u64)>> = par_chunks(src, opts, |base, data, own| {
        let mut out = Vec::new();
        let limit = own.min(data.len());
        let mut j = 0usize;
        while j < limit {
            if let Some(key) = key_at(data, j)
                && let Some(list) = table.get(&key)
            {
                for &(pi, si) in list {
                    out.push((pi, si, base + j as u64));
                }
            }
            j += 4;
        }
        out
    });
    let mut per: Vec<AHashMap<u64, Vec<usize>>> = vec![AHashMap::new(); probes.len()];
    for list in hits {
        for (pi, si, addr) in list {
            let off = probes[pi].samples[si].0 as u64;
            if addr >= off {
                per[pi].entry(addr - off).or_default().push(si);
            }
        }
    }
    let mut out = Vec::new();
    for (pi, p) in probes.iter().enumerate() {
        let total = p.samples.len();
        let mut best: Option<(u64, usize)> = None;
        for (base, sis) in &per[pi] {
            let mut u = sis.clone();
            u.sort_unstable();
            u.dedup();
            let better = match best {
                None => true,
                Some((bb, bc)) => u.len() > bc || (u.len() == bc && *base < bb),
            };
            if better {
                best = Some((*base, u.len()));
            }
        }
        let Some((base, hit)) = best else {
            out.push(LumpCoverage {
                lump: p.lump,
                len: p.len,
                samples_total: total,
                samples_hit: 0,
                base: None,
                match_percent: 0.0,
                state: Coverage::Absent,
            });
            continue;
        };
        let mem = src.read_vec(base, p.len);
        let percent = match file_lump(p.lump) {
            Some(file) => compare_percent(&mem, &file, p.plane_like),
            None => 0.0,
        };
        let state = if mem.len() == p.len && percent >= 99.99 {
            Coverage::Verified
        } else if percent >= 60.0 {
            Coverage::Mostly
        } else if hit >= 2 {
            Coverage::Partial
        } else {
            Coverage::Absent
        };
        out.push(LumpCoverage {
            lump: p.lump,
            len: p.len,
            samples_total: total,
            samples_hit: hit,
            base: Some(base),
            match_percent: percent,
            state,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    fn noise(n: usize, seed: u32) -> Vec<u8> {
        let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 8) as u8 | 0x80
            })
            .collect()
    }

    fn sample_entities(n: usize) -> String {
        let mut t =
            String::from("{\n\"world_maxs\" \"1 2 3\"\n\"classname\" \"worldspawn\"\n\"mapversion\" \"7\"\n}\n");
        for i in 0..n {
            t.push_str(&format!(
                "{{\n\"origin\" \"{i} 0 0\"\n\"classname\" \"info_target\"\n\"targetname\" \"t{i}\"\n}}\n"
            ));
        }
        t
    }

    fn mem_with(prefix: usize, text: &str, base: u64) -> VecMem {
        let mut block = noise(prefix, 1);
        block.push(0);
        block.extend_from_slice(text.as_bytes());
        block.push(0);
        block.extend_from_slice(&noise(5000, 2));
        let mut m = VecMem::default();
        m.add(base, block);
        m
    }

    #[test]
    fn entity_lump_is_found_whole() {
        let text = sample_entities(50);
        let m = mem_with(3000, &text, 0x10_0000);
        let hits = find_entity_lumps(&m, &ScanOptions::default());
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].text, text);
        assert_eq!(hits[0].entities, 51);
    }

    #[test]
    fn entity_lump_found_across_chunk_borders() {
        let text = sample_entities(40);
        let m = mem_with(100_000, &text, 0x20_0000);
        let opts = ScanOptions { chunk: 4096, overlap: 64, ..ScanOptions::default() };
        let hits = find_entity_lumps(&m, &opts);
        assert_eq!(hits.len(), 1, "{}", hits.len());
        assert_eq!(hits[0].text, text);
    }

    #[test]
    fn lua_mentions_of_worldspawn_are_not_maps() {
        let mut m = VecMem::default();
        let mut block = noise(500, 3);
        block.extend_from_slice(b"\0print(\"classname\" \"worldspawn\") local x = 1\0");
        block.extend_from_slice(&noise(500, 4));
        m.add(0x1000, block);
        assert!(find_entity_lumps(&m, &ScanOptions::default()).is_empty());
    }

    #[test]
    fn the_bigger_stale_copy_wins_and_copies_merge() {
        let big = sample_entities(80);
        let small = sample_entities(10);
        let mut m = mem_with(100, &big, 0x1000_0000);
        let other = mem_with(100, &small, 0x2000_0000);
        m.blocks.extend(other.blocks);
        let twin = mem_with(7000, &big, 0x3000_0000);
        m.blocks.extend(twin.blocks);
        let hits = find_entity_lumps(&m, &ScanOptions::default());
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].entities, 81);
        assert_eq!(hits[1].entities, 11);
    }

    #[test]
    fn chunk_plan_covers_regions_without_gaps() {
        let regions = [
            Region { base: 0x1000, size: 100, writable: true, image: false },
            Region { base: 0x9000, size: 1000, writable: false, image: false },
            Region { base: 0xf000, size: 50, writable: true, image: true },
        ];
        let opts = ScanOptions { chunk: 300, overlap: 20, writable_only: false, min_region: 0 };
        let plan = plan_chunks(&regions, &opts);
        assert_eq!(plan[0], (0x1000, 100));
        let second: Vec<_> = plan.iter().filter(|(b, _)| *b >= 0x9000).collect();
        assert_eq!(second.len(), 4);
        assert_eq!(second[0], &(0x9000, 320));
        assert_eq!(second[3].0 + second[3].1, 0x9000 + 1000);
        let only = plan_chunks(&regions, &ScanOptions { writable_only: true, ..opts });
        assert_eq!(only.len(), 1);
    }

    #[test]
    fn lumps_are_recognised_in_memory() {
        let vertexes: Vec<u8> = noise(12 * 400, 9);
        let visibility: Vec<u8> = noise(2000, 10);
        let mut planes = Vec::new();
        for i in 0..300u32 {
            planes.extend_from_slice(&(1.0f32 - i as f32 * 0.001).to_le_bytes());
            planes.extend_from_slice(&(i as f32 * 3.5).to_le_bytes());
            planes.extend_from_slice(&0f32.to_le_bytes());
            planes.extend_from_slice(&(100.0f32 + i as f32).to_le_bytes());
            planes.extend_from_slice(&0i32.to_le_bytes());
        }
        let data = hp_bsp::synthetic_bsp(
            20,
            &[
                (hp_bsp::lump::ENTITIES, b"\0".to_vec()),
                (1, planes.clone()),
                (3, vertexes.clone()),
                (4, visibility.clone()),
            ],
        );
        let bsp = Bsp::from_vec(data).expect("bsp");
        let probes = probes_from_bsp(&bsp, &[hp_bsp::lump::ENTITIES]);
        assert_eq!(probes.len(), 3);

        let mut runtime_planes = planes.clone();
        for rec in runtime_planes.chunks_mut(20) {
            rec[16] = 3;
            rec[17] = 5;
            rec[18] = 0xaa;
            rec[19] = 0xbb;
        }
        let mut m = VecMem::default();
        let mut a = noise(776, 20);
        a.extend_from_slice(&runtime_planes);
        a.extend_from_slice(&noise(333, 21));
        m.add(0x10_0000, a);
        let mut b = noise(1000, 22);
        let vert_at = b.len() as u64;
        b.extend_from_slice(&vertexes);
        let mut damaged = visibility.clone();
        for k in (0..damaged.len()).step_by(7) {
            damaged[k] ^= 0xff;
        }
        b.extend_from_slice(&damaged);
        m.add(0x80_0000, b);

        let lumps = |i: usize| bsp.lump(i).ok().map(|c| c.into_owned());
        let cov = scan_probes(&m, &probes, &lumps, &ScanOptions { chunk: 2048, ..ScanOptions::default() });
        let by = |l: usize| cov.iter().find(|c| c.lump == l).expect("lump");
        assert_eq!(by(1).state, Coverage::Verified);
        assert_eq!(by(1).base, Some(0x10_0000 + 776));
        assert_eq!(by(3).state, Coverage::Verified);
        assert_eq!(by(3).base, Some(0x80_0000 + vert_at));
        assert!(matches!(by(4).state, Coverage::Partial | Coverage::Mostly | Coverage::Absent), "{:?}", by(4));
        assert_ne!(by(4).state, Coverage::Verified);
    }

    #[test]
    fn dull_data_gives_no_samples() {
        assert!(probe_lump(5, &[0u8; 400], 8).samples.is_empty());
        assert!(probe_lump(5, &[1u8; 400], 8).samples.is_empty());
        assert!(probe_lump(5, &[1u8; 8], 8).samples.is_empty());
        let good = probe_lump(5, &noise(1000, 5), 8);
        assert!(good.samples.len() >= 6);
    }
}
