use hp_bsp::Bsp;

use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

pub const BRUSH_RECORD: usize = 12;
pub const SIDE_RECORD: usize = 16;
pub const BOX_RECORD: usize = 48;
pub const PLANE_RECORD: usize = 20;
pub const SURFACE_RECORD: usize = 16;
const WINDOW: usize = 64;
const MAX_SIDES_PER_BRUSH: i32 = 128;
const REACH: usize = 400_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrushRec {
    pub contents: i32,
    pub numsides: i32,
    pub index: i32,
}

impl BrushRec {
    pub fn axial(&self) -> bool {
        self.numsides == -1
    }

    pub fn plausible(&self) -> bool {
        self.contents >= 0
            && (self.numsides == -1 || (4..=MAX_SIDES_PER_BRUSH).contains(&self.numsides))
            && self.index >= 0
    }
}

pub fn parse_brushes(data: &[u8]) -> Vec<BrushRec> {
    data.chunks_exact(BRUSH_RECORD)
        .map(|c| BrushRec {
            contents: i32::from_le_bytes([c[0], c[1], c[2], c[3]]),
            numsides: i32::from_le_bytes([c[4], c[5], c[6], c[7]]),
            index: i32::from_le_bytes([c[8], c[9], c[10], c[11]]),
        })
        .collect()
}

#[derive(Default)]
struct Chain {
    prev_axial: Option<i32>,
    next_first: Option<i32>,
    axial: usize,
    solid: usize,
}

impl Chain {
    fn accept(&mut self, r: &BrushRec) -> bool {
        if !r.plausible() {
            return false;
        }
        if r.axial() {
            if self.prev_axial.is_some_and(|p| r.index != p + 1) {
                return false;
            }
            self.prev_axial = Some(r.index);
            self.axial += 1;
        } else {
            if self.next_first.is_some_and(|n| r.index != n) {
                return false;
            }
            self.next_first = Some(r.index + r.numsides);
            self.solid += 1;
        }
        true
    }
}

pub fn chain_window(recs: &[BrushRec]) -> bool {
    chain_window_w(recs, WINDOW)
}

pub fn chain_window_w(recs: &[BrushRec], window: usize) -> bool {
    let mut c = Chain::default();
    let need = if window >= WINDOW { 4 } else { 2 };
    recs.iter().all(|r| c.accept(r))
        && c.axial + c.solid >= window
        && (c.axial >= need || c.solid >= need)
        && (window < WINDOW || (c.axial >= 4 && c.solid >= 4))
}

pub fn longest_run_around(recs: &[BrushRec], at: usize) -> (usize, usize) {
    longest_run_around_w(recs, at, WINDOW)
}

pub fn longest_run_around_w(recs: &[BrushRec], at: usize, window: usize) -> (usize, usize) {
    let mut start = at;
    while start > 0 {
        let mut c = Chain::default();
        let mut ok = true;
        let end = (start + window).min(recs.len());
        for r in &recs[start - 1..end] {
            if !c.accept(r) {
                ok = false;
                break;
            }
        }
        if !ok {
            break;
        }
        start -= 1;
    }
    let mut c = Chain::default();
    let mut end = start;
    for r in &recs[start..] {
        if !c.accept(r) {
            break;
        }
        end += 1;
    }
    (start, end)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CmFindings {
    pub brush_addr: u64,
    pub brushes: Vec<BrushRec>,
    pub planes_addr: u64,
    pub planes: usize,
    pub sides_addr: u64,
    pub sides: Vec<Side>,
    pub boxes_addr: u64,
    pub boxes: Vec<AxialBox>,
    pub surface_addr: Option<u64>,
    pub surface_names: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Side {
    pub plane: u32,
    pub surface: u16,
    pub bevel: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxialBox {
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub surfaces: [u16; 6],
}

impl CmFindings {
    pub fn axial_count(&self) -> usize {
        self.brushes.iter().filter(|b| b.axial()).count()
    }

    pub fn total_sides(&self) -> usize {
        self.sides.len() + self.boxes.len() * 6
    }
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn u32_at(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

pub fn valid_plane(rec: &[u8]) -> bool {
    if rec.len() < PLANE_RECORD {
        return false;
    }
    let n = [f32_at(rec, 0), f32_at(rec, 4), f32_at(rec, 8)];
    let dist = f32_at(rec, 12);
    if !n.iter().all(|v| v.is_finite()) || !dist.is_finite() || dist.abs() > 1.0e6 {
        return false;
    }
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if !(0.99..=1.01).contains(&len) {
        return false;
    }
    let kind = rec[16];
    if kind > 5 {
        return false;
    }
    if kind < 3 {
        return (n[kind as usize].abs() - 1.0).abs() < 1.0e-4;
    }
    let top = n.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    n[(kind - 3) as usize].abs() >= top - 1.0e-4
}

pub fn count_planes_before<S: MemSource + ?Sized>(src: &S, end: u64) -> (u64, usize) {
    let mut best = count_planes_run(src, end);
    for pad in [4u64, 8, 12] {
        if best.1 > 0 || end < pad {
            break;
        }
        best = count_planes_run(src, end - pad);
    }
    best
}

fn count_planes_run<S: MemSource + ?Sized>(src: &S, end: u64) -> (u64, usize) {
    let mut count = 0usize;
    let mut base = end;
    let step = 4096usize;
    loop {
        if base < (step * PLANE_RECORD) as u64 {
            break;
        }
        let from = base - (step * PLANE_RECORD) as u64;
        let data = src.read_vec(from, step * PLANE_RECORD);
        if data.len() != step * PLANE_RECORD {
            let mut got = 0usize;
            let mut b = base;
            while b >= PLANE_RECORD as u64 {
                let rec = src.read_vec(b - PLANE_RECORD as u64, PLANE_RECORD);
                if rec.len() == PLANE_RECORD && valid_plane(&rec) {
                    got += 1;
                    b -= PLANE_RECORD as u64;
                } else {
                    break;
                }
            }
            return (b, count + got);
        }
        let mut valid = 0usize;
        for rec in data.chunks_exact(PLANE_RECORD).rev() {
            if valid_plane(rec) {
                valid += 1;
            } else {
                break;
            }
        }
        count += valid;
        base -= (valid * PLANE_RECORD) as u64;
        if valid < step {
            break;
        }
    }
    (base, count)
}

fn read_string<S: MemSource + ?Sized>(src: &S, addr: u64) -> Option<String> {
    let data = src.read_vec(addr, 260);
    let end = data.iter().position(|b| *b == 0)?;
    let s = &data[..end];
    if s.is_empty() || !s.iter().all(|b| (0x20..0x7f).contains(b)) {
        return None;
    }
    Some(String::from_utf8_lossy(s).into_owned())
}

pub fn find_brush_arrays<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> Vec<u64> {
    find_brush_arrays_w(src, opts, WINDOW)
}

pub fn find_brush_arrays_w<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions, window: usize) -> Vec<u64> {
    let opts = ScanOptions { overlap: BRUSH_RECORD * (window + 2), writable_only: true, ..*opts };
    let mut found: Vec<u64> = par_chunks(src, &opts, |base, data, own| {
        let mut out = Vec::new();
        let limit = own.min(data.len());
        let mut j = 0usize;
        while j + BRUSH_RECORD * window <= data.len() && j < limit {
            let c = &data[j..];
            let first = BrushRec {
                contents: i32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                numsides: i32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                index: i32::from_le_bytes([c[8], c[9], c[10], c[11]]),
            };
            if first.plausible() && first.index < 5_000_000 {
                let recs = parse_brushes(&c[..BRUSH_RECORD * window]);
                if chain_window_w(&recs, window) {
                    out.push(base + j as u64);
                    j += BRUSH_RECORD * window;
                    continue;
                }
            }
            j += 4;
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    found.sort_unstable();
    found
}

pub fn span_of<S: MemSource + ?Sized>(src: &S, addr: u64) -> Option<(u64, u64)> {
    let mut regions = src.regions();
    regions.sort_by_key(|r| r.base);
    let i = regions.iter().position(|r| addr >= r.base && addr < r.base + r.size)?;
    let mut lo = regions[i].base;
    let mut hi = regions[i].base + regions[i].size;
    let mut k = i;
    while k > 0 && regions[k - 1].base + regions[k - 1].size == lo {
        k -= 1;
        lo = regions[k].base;
    }
    let mut k = i;
    while k + 1 < regions.len() && regions[k + 1].base == hi {
        k += 1;
        hi = regions[k].base + regions[k].size;
    }
    Some((lo, hi))
}

pub fn window_around<S: MemSource + ?Sized>(src: &S, addr: u64, before: u64, after: u64) -> Option<(u64, Vec<u8>)> {
    let (lo, hi) = span_of(src, addr)?;
    let from = addr.saturating_sub(before).max(lo);
    let to = (addr + after).min(hi);
    Some((from, src.read_vec(from, (to - from) as usize)))
}

pub fn read_brush_array<S: MemSource + ?Sized>(src: &S, hit: u64) -> Option<(u64, Vec<BrushRec>)> {
    read_brush_array_w(src, hit, WINDOW)
}

pub fn read_brush_array_w<S: MemSource + ?Sized>(src: &S, hit: u64, window: usize) -> Option<(u64, Vec<BrushRec>)> {
    let (from, data) = window_around(src, hit, (REACH * BRUSH_RECORD) as u64, (REACH * BRUSH_RECORD) as u64)?;
    let skew = ((hit - from) % BRUSH_RECORD as u64) as usize;
    let from = from + skew as u64;
    let data = &data[skew.min(data.len())..];
    let recs = parse_brushes(data);
    let before = ((hit - from) / BRUSH_RECORD as u64) as usize;
    if recs.len() <= before {
        return None;
    }
    let (s, e) = longest_run_around_w(&recs, before, window);
    if e - s < window {
        return None;
    }
    Some((from + (s * BRUSH_RECORD) as u64, recs[s..e].to_vec()))
}

pub fn read_sides<S: MemSource + ?Sized>(
    src: &S,
    addr: u64,
    count: usize,
    planes_base: u64,
    planes: usize,
) -> Option<Vec<Side>> {
    let data = src.read_vec(addr, count * SIDE_RECORD);
    if data.len() != count * SIDE_RECORD {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for rec in data.chunks_exact(SIDE_RECORD) {
        let ptr = u64::from_le_bytes([rec[0], rec[1], rec[2], rec[3], rec[4], rec[5], rec[6], rec[7]]);
        if ptr < planes_base {
            return None;
        }
        let off = ptr - planes_base;
        if !off.is_multiple_of(PLANE_RECORD as u64) || off / PLANE_RECORD as u64 >= planes as u64 {
            return None;
        }
        out.push(Side {
            plane: (off / PLANE_RECORD as u64) as u32,
            surface: u16::from_le_bytes([rec[8], rec[9]]),
            bevel: u16::from_le_bytes([rec[10], rec[11]]),
        });
    }
    Some(out)
}

pub fn read_boxes<S: MemSource + ?Sized>(src: &S, addr: u64, count: usize) -> Option<Vec<AxialBox>> {
    let data = src.read_vec(addr, count * BOX_RECORD);
    if data.len() != count * BOX_RECORD {
        return None;
    }
    let mut out = Vec::with_capacity(count);
    for rec in data.chunks_exact(BOX_RECORD) {
        let mins = [f32_at(rec, 0), f32_at(rec, 4), f32_at(rec, 8)];
        let maxs = [f32_at(rec, 16), f32_at(rec, 20), f32_at(rec, 24)];
        if !mins.iter().chain(&maxs).all(|v| v.is_finite()) || (0..3).any(|k| mins[k] > maxs[k]) {
            return None;
        }
        let mut surfaces = [0u16; 6];
        for (k, s) in surfaces.iter_mut().enumerate() {
            *s = u16::from_le_bytes([rec[32 + k * 2], rec[33 + k * 2]]);
        }
        out.push(AxialBox { mins, maxs, surfaces });
    }
    Some(out)
}

fn surface_candidates(base: u64, data: &[u8], own: usize, run: usize) -> Vec<u64> {
    let mut out = Vec::new();
    let limit = own.min(data.len());
    let mut j = 0usize;
    while j + SURFACE_RECORD * run <= data.len() && j < limit {
        let p0 = u64::from(u32_at(data, j));
        if p0 > 0x1_0000 {
            let mut ok = true;
            for k in 0..run - 1 {
                let a = u64::from(u32_at(data, j + k * SURFACE_RECORD));
                let b = u64::from(u32_at(data, j + (k + 1) * SURFACE_RECORD));
                if !(b > a && b - a >= 2 && b - a <= 300) {
                    ok = false;
                    break;
                }
            }
            if ok {
                out.push(base + j as u64);
                j += SURFACE_RECORD * run;
                continue;
            }
        }
        j += 4;
    }
    out
}

fn near_candidates(base: u64, data: &[u8], own: usize, run: usize) -> Vec<u64> {
    let mut out = Vec::new();
    let limit = own.min(data.len());
    let mut j = 0usize;
    while j + SURFACE_RECORD * run <= data.len() && j < limit {
        let p0 = u64::from(u32_at(data, j));
        if p0 > 0x1_0000 && u32_at(data, j + 4) == 0 {
            let ok = (1..run).all(|k| {
                let at = j + k * SURFACE_RECORD;
                let p = u64::from(u32_at(data, at));
                u32_at(data, at + 4) == 0 && p > 0x1_0000 && p.abs_diff(p0) <= SURFACE_BLOB_REACH
            });
            if ok {
                out.push(base + j as u64);
                j += SURFACE_RECORD * run;
                continue;
            }
        }
        j += 4;
    }
    out
}

const SURFACE_BLOB_REACH: u64 = 1 << 20;

fn name_like(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'/' | b'-' | b'.' | b' ' | b'+' | b'(' | b')'))
}

pub fn find_surface_array<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    min_count: usize,
    near: Option<u64>,
) -> Option<(u64, Vec<String>)> {
    find_surface_array_ordered(src, opts, min_count, near)
        .or_else(|| find_surface_array_any(src, opts, min_count, near))
}

fn find_surface_array_any<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    min_count: usize,
    near: Option<u64>,
) -> Option<(u64, Vec<String>)> {
    let run = min_count.clamp(6, 24);
    let mut cands: Vec<u64> = match near {
        Some(anchor) => {
            let (from, data) = window_around(src, anchor.saturating_sub(1), 2 << 20, 0)?;
            let skew = ((anchor - from) % SURFACE_RECORD as u64) as usize;
            near_candidates(from + skew as u64, &data[skew.min(data.len())..], usize::MAX, run)
        }
        None => {
            let opts = ScanOptions { overlap: SURFACE_RECORD * 40, writable_only: true, ..*opts };
            par_chunks(src, &opts, |b, d, o| near_candidates(b, d, o, run)).into_iter().flatten().collect()
        }
    };
    cands.sort_unstable();
    for hit in cands {
        let Some((from, data)) =
            window_around(src, hit, (4096 * SURFACE_RECORD) as u64, (20_000 * SURFACE_RECORD) as u64)
        else {
            continue;
        };
        let skew = ((hit - from) % SURFACE_RECORD as u64) as usize;
        let from = from + skew as u64;
        let data = data[skew.min(data.len())..].to_vec();
        let before = ((hit - from) / SURFACE_RECORD as u64) as usize;
        let n = data.len() / SURFACE_RECORD;
        let p0 = u64::from(u32_at(&data, before.min(n.saturating_sub(1)) * SURFACE_RECORD));
        let name_at = |i: usize| -> Option<String> {
            let at = i * SURFACE_RECORD;
            let p = u64::from(u32_at(&data, at));
            if u32_at(&data, at + 4) != 0 || p <= 0x1_0000 || p.abs_diff(p0) > SURFACE_BLOB_REACH {
                return None;
            }
            read_string(src, p).filter(|s| name_like(s))
        };
        let mut start = before.min(n.saturating_sub(1));
        while start > 0 && name_at(start - 1).is_some() {
            start -= 1;
        }
        let mut names = Vec::new();
        let mut i = start;
        while i < n {
            match name_at(i) {
                Some(s) => names.push(s),
                None => break,
            }
            i += 1;
        }
        if names.len() >= min_count {
            return Some((from + (start * SURFACE_RECORD) as u64, names));
        }
    }
    None
}

fn find_surface_array_ordered<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    min_count: usize,
    near: Option<u64>,
) -> Option<(u64, Vec<String>)> {
    let run = min_count.clamp(6, 24);
    let mut cands: Vec<u64> = match near {
        Some(anchor) => {
            let (from, data) = window_around(src, anchor.saturating_sub(1), 2 << 20, 0)?;
            let skew = ((anchor - from) % SURFACE_RECORD as u64) as usize;
            surface_candidates(from + skew as u64, &data[skew.min(data.len())..], usize::MAX, run)
        }
        None => {
            let opts = ScanOptions { overlap: SURFACE_RECORD * 40, writable_only: true, ..*opts };
            par_chunks(src, &opts, |b, d, o| surface_candidates(b, d, o, run)).into_iter().flatten().collect()
        }
    };
    cands.sort_unstable();
    for hit in cands {
        let Some((from, data)) =
            window_around(src, hit, (4096 * SURFACE_RECORD) as u64, (20_000 * SURFACE_RECORD) as u64)
        else {
            continue;
        };
        let skew = ((hit - from) % SURFACE_RECORD as u64) as usize;
        let from = from + skew as u64;
        let data = data[skew.min(data.len())..].to_vec();
        let before = ((hit - from) / SURFACE_RECORD as u64) as usize;
        let n = data.len() / SURFACE_RECORD;
        let ptr = |i: usize| u64::from(u32_at(&data, i * SURFACE_RECORD));
        let name_ok = |i: usize, next: u64| -> Option<String> {
            let s = read_string(src, ptr(i))?;
            (ptr(i) + s.len() as u64 + 1 == next).then_some(s)
        };
        let mut start = before.min(n.saturating_sub(1));
        while start > 0 {
            let i = start - 1;
            if name_ok(i, ptr(i + 1)).is_some() {
                start -= 1;
            } else {
                break;
            }
        }
        let mut names = Vec::new();
        let mut i = start;
        while i + 1 < n {
            match name_ok(i, ptr(i + 1)) {
                Some(s) => names.push(s),
                None => break,
            }
            i += 1;
        }
        if let Some(last) = read_string(src, ptr(i)) {
            names.push(last);
        }
        if names.len() >= min_count {
            return Some((from + (start * SURFACE_RECORD) as u64, names));
        }
    }
    None
}

pub fn find_cm_geometry<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> Option<CmFindings> {
    for window in SMALL_WINDOWS {
        if let Some(found) = find_cm_geometry_w(src, opts, window)
            && (window == WINDOW
                || (found.planes > 0 && !found.sides.is_empty()
                    || found.brushes.iter().all(|b| b.axial()) && !found.boxes.is_empty()))
        {
            return Some(found);
        }
    }
    None
}

const SMALL_WINDOWS: [usize; 4] = [WINDOW, 32, 16, 8];

pub fn find_cm_geometry_w<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions, window: usize) -> Option<CmFindings> {
    let hits = find_brush_arrays_w(src, opts, window);
    let mut best: Option<CmFindings> = None;
    let mut covered: Vec<(u64, u64)> = Vec::new();
    for hit in hits {
        if covered.iter().any(|&(lo, hi)| hit >= lo && hit < hi) {
            continue;
        }
        let Some((addr, brushes)) = read_brush_array_w(src, hit, window) else { continue };
        covered.push((addr, addr + (brushes.len() * BRUSH_RECORD) as u64));
        if best.as_ref().is_some_and(|b| b.brushes.len() >= brushes.len()) {
            continue;
        }
        let end = addr + (brushes.len() * BRUSH_RECORD) as u64;
        let (planes_addr, planes) = count_planes_before(src, addr);
        let solid_sides: usize = brushes.iter().filter(|b| !b.axial()).map(|b| b.numsides as usize).sum();
        let axial = brushes.iter().filter(|b| b.axial()).count();
        let sides_addr = end.div_ceil(16) * 16;
        let sides = if planes > 0 { read_sides(src, sides_addr, solid_sides, planes_addr, planes) } else { None };
        let (sides, sides_ok) = match sides {
            Some(s) => (s, true),
            None => (Vec::new(), false),
        };
        let sides_end = sides_addr + (solid_sides.max(1) * SIDE_RECORD) as u64;
        let boxes_addr = sides_end.div_ceil(16) * 16;
        let boxes = if sides_ok { read_boxes(src, boxes_addr, axial).unwrap_or_default() } else { Vec::new() };
        best = Some(CmFindings {
            brush_addr: addr,
            brushes,
            planes_addr,
            planes,
            sides_addr,
            sides,
            boxes_addr,
            boxes,
            surface_addr: None,
            surface_names: Vec::new(),
        });
    }
    best
}

pub fn attach_surfaces<S: MemSource + ?Sized>(src: &S, found: &mut CmFindings, near: Option<u64>, opts: &ScanOptions) {
    let max_surface = found
        .sides
        .iter()
        .map(|s| usize::from(s.surface))
        .chain(found.boxes.iter().flat_map(|b| b.surfaces.iter().map(|s| usize::from(*s))))
        .max()
        .unwrap_or(0);
    if let Some((addr, names)) = find_surface_array(src, opts, max_surface + 1, near) {
        found.surface_addr = Some(addr);
        found.surface_names = names;
    }
}

pub fn find_cm<S: MemSource + ?Sized>(src: &S, opts: &ScanOptions) -> Option<CmFindings> {
    let mut found = find_cm_geometry(src, opts)?;
    attach_surfaces(src, &mut found, None, opts);
    Some(found)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SideKinds {
    pub nodraw: usize,
    pub tools: usize,
    pub sky: usize,
    pub clip: usize,
    pub trigger: usize,
    pub other: usize,
}

pub fn classify(name: &str, kinds: &mut SideKinds) {
    let n = name.to_ascii_lowercase();
    if n.ends_with("toolsnodraw") {
        kinds.nodraw += 1;
    } else if n.ends_with("toolsskybox") || n.starts_with("skybox/") {
        kinds.sky += 1;
    } else if n.contains("toolsclip") || n.contains("toolsplayerclip") || n.contains("toolsnpcclip") {
        kinds.clip += 1;
    } else if n.contains("toolstrigger") {
        kinds.trigger += 1;
    } else if n.starts_with("tools/") {
        kinds.tools += 1;
    } else {
        kinds.other += 1;
    }
}

impl CmFindings {
    pub fn side_kinds(&self) -> SideKinds {
        let mut k = SideKinds::default();
        if self.surface_names.is_empty() {
            return k;
        }
        let name = |s: u16| self.surface_names.get(usize::from(s)).map(String::as_str).unwrap_or("");
        for s in &self.sides {
            classify(name(s.surface), &mut k);
        }
        for b in &self.boxes {
            for s in b.surfaces {
                classify(name(s), &mut k);
            }
        }
        k
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CmComparison {
    pub brushes_compared: usize,
    pub brush_mismatch: usize,
    pub sides_compared: usize,
    pub side_plane_mismatch: usize,
    pub material_mismatch_brushes: usize,
    pub boxes_compared: usize,
    pub box_mismatch: usize,
}

impl CmComparison {
    pub fn identical(&self) -> bool {
        self.brush_mismatch == 0
            && self.side_plane_mismatch == 0
            && self.material_mismatch_brushes == 0
            && self.box_mismatch == 0
            && self.brushes_compared > 0
    }
}

pub fn compare_with_bsp(cm: &CmFindings, bsp: &Bsp) -> CmComparison {
    let mut c = CmComparison::default();
    let (Ok(brushes), Ok(sides), Ok(planes)) = (bsp.lump(18), bsp.lump(19), bsp.lump(1)) else { return c };
    let texinfo = bsp.lump(6).map(|l| l.into_owned()).unwrap_or_default();
    let texdata = bsp.lump(2).map(|l| l.into_owned()).unwrap_or_default();
    let names = bsp.texture_names().unwrap_or_default();
    let file_material = |ti: i16| -> Option<String> {
        let t = texinfo.chunks_exact(72).nth(usize::try_from(ti).ok()?)?;
        let td = i32::from_le_bytes([t[68], t[69], t[70], t[71]]);
        let d = texdata.chunks_exact(32).nth(usize::try_from(td).ok()?)?;
        let id = i32::from_le_bytes([d[12], d[13], d[14], d[15]]);
        names.get(usize::try_from(id).ok()?).map(|n| n.to_ascii_lowercase())
    };
    let plane_at = |i: usize| -> Option<[f32; 4]> {
        let p = planes.chunks_exact(PLANE_RECORD).nth(i)?;
        Some([f32_at(p, 0), f32_at(p, 4), f32_at(p, 8), f32_at(p, 12)])
    };
    let mem_material = |s: u16| cm.surface_names.get(usize::from(s)).map(|n| n.to_ascii_lowercase());
    let file_brushes: Vec<BrushRec> = brushes
        .chunks_exact(12)
        .map(|b| BrushRec {
            contents: i32::from_le_bytes([b[8], b[9], b[10], b[11]]),
            numsides: i32::from_le_bytes([b[4], b[5], b[6], b[7]]),
            index: i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        })
        .collect();
    let file_brushes: Vec<BrushRec> = {
        let firsts: Vec<i32> = file_brushes.iter().map(|b| b.index).collect();
        let total = i32::try_from(sides.len() / 8).unwrap_or(i32::MAX);
        file_brushes
            .iter()
            .enumerate()
            .map(|(k, b)| {
                let mut b = *b;
                if b.numsides == 0 && b.index >= 0 {
                    b.numsides = firsts.get(k + 1).copied().unwrap_or(total) - b.index;
                }
                b
            })
            .collect()
    };
    for (i, m) in cm.brushes.iter().enumerate() {
        let Some(f) = file_brushes.get(i) else {
            c.brush_mismatch += 1;
            continue;
        };
        c.brushes_compared += 1;
        let sides_match = if m.axial() { f.numsides == 6 } else { f.numsides == m.numsides };
        if f.contents != m.contents || !sides_match {
            c.brush_mismatch += 1;
        }
    }
    let mut axial_seen = 0usize;
    for (i, m) in cm.brushes.iter().enumerate() {
        let Some(f) = file_brushes.get(i) else { continue };
        let first = f.index.max(0) as usize;
        if m.axial() {
            let Some(b) = cm.boxes.get(axial_seen) else { continue };
            axial_seen += 1;
            c.boxes_compared += 1;
            let mut ok = f.numsides == 6;
            if ok {
                let order: [([f32; 3], f32); 6] = [
                    ([-1.0, 0.0, 0.0], -b.mins[0]),
                    ([0.0, -1.0, 0.0], -b.mins[1]),
                    ([0.0, 0.0, -1.0], -b.mins[2]),
                    ([1.0, 0.0, 0.0], b.maxs[0]),
                    ([0.0, 1.0, 0.0], b.maxs[1]),
                    ([0.0, 0.0, 1.0], b.maxs[2]),
                ];
                for (want_n, want_d) in order {
                    let found = (0..6).any(|k| {
                        let off = (first + k) * 8;
                        sides.get(off..off + 2).is_some_and(|s| {
                            plane_at(usize::from(u16::from_le_bytes([s[0], s[1]]))).is_some_and(|p| {
                                p[0] == want_n[0] && p[1] == want_n[1] && p[2] == want_n[2] && p[3] == want_d
                            })
                        })
                    });
                    if !found {
                        ok = false;
                    }
                }
                let mut mem_names: Vec<String> = b.surfaces.iter().filter_map(|s| mem_material(*s)).collect();
                let mut file_names: Vec<String> = (0..6)
                    .filter_map(|k| {
                        let off = (first + k) * 8;
                        let s = sides.get(off..off + 8)?;
                        file_material(i16::from_le_bytes([s[2], s[3]]))
                    })
                    .collect();
                mem_names.sort();
                file_names.sort();
                if !cm.surface_names.is_empty() && mem_names != file_names {
                    c.material_mismatch_brushes += 1;
                }
            }
            if !ok {
                c.box_mismatch += 1;
            }
        } else {
            let base = m.index.max(0) as usize;
            let mut mem_names: Vec<Option<String>> = Vec::new();
            let mut file_names: Vec<Option<String>> = Vec::new();
            for k in 0..m.numsides as usize {
                let Some(ms) = cm.sides.get(base + k) else { continue };
                let off = (first + k) * 8;
                let Some(fs) = sides.get(off..off + 8) else {
                    c.side_plane_mismatch += 1;
                    continue;
                };
                c.sides_compared += 1;
                if u32::from(u16::from_le_bytes([fs[0], fs[1]])) != ms.plane {
                    c.side_plane_mismatch += 1;
                }
                if !cm.surface_names.is_empty() {
                    file_names.push(file_material(i16::from_le_bytes([fs[2], fs[3]])));
                    mem_names.push(mem_material(ms.surface));
                }
            }
            mem_names.sort();
            file_names.sort();
            if mem_names != file_names {
                c.material_mismatch_brushes += 1;
            }
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    #[test]
    fn planes_are_counted_across_the_alignment_padding_before_the_brush_table() {
        for pad in [0usize, 4, 8, 12] {
            let mut mem = VecMem::default();
            let mut data = vec![0u8; 64];
            for i in 0..50 {
                let d = i as f32;
                data.extend(plane_bytes([0.0, 0.0, 1.0], d, 2));
            }
            data.extend(vec![0u8; pad]);
            let end = 0x4000_0000u64 + data.len() as u64;
            mem.add(0x4000_0000, data);
            let (start, count) = count_planes_before(&mem, end);
            assert_eq!(count, 50, "pad {pad}");
            assert_eq!(start, 0x4000_0000 + 64, "pad {pad}");
        }
    }

    pub fn plane_bytes(n: [f32; 3], d: f32, kind: u8) -> Vec<u8> {
        let mut v = Vec::new();
        for x in n {
            v.extend_from_slice(&x.to_le_bytes());
        }
        v.extend_from_slice(&d.to_le_bytes());
        v.extend_from_slice(&[kind, 0, 0, 0]);
        v
    }

    struct Layout {
        mem: VecMem,
        brush_addr: u64,
        axial: usize,
        solid_sides: usize,
        brushes: usize,
    }

    fn layout() -> Layout {
        let base: u64 = 0x2000_0000;
        let mut data = Vec::new();
        let mut planes = Vec::new();
        for i in 0..200 {
            let d = i as f32 * 8.0;
            planes.extend(plane_bytes([1.0, 0.0, 0.0], d, 0));
            planes.extend(plane_bytes([-1.0, 0.0, 0.0], -d, 0));
            planes.extend(plane_bytes([0.0, 1.0, 0.0], d, 1));
            planes.extend(plane_bytes([0.0, 0.0, 1.0], d, 2));
        }
        let planes_n = planes.len() / PLANE_RECORD;
        data.extend_from_slice(&[0xabu8; 64]);
        let planes_addr = base + data.len() as u64;
        data.extend_from_slice(&planes);
        let brush_addr = base + data.len() as u64;
        let mut brushes = Vec::new();
        let mut axial_i = 0i32;
        let mut first = 0i32;
        let mut solid_sides = 0usize;
        let count = 300usize;
        let mut axial = 0usize;
        for i in 0..count {
            let is_axial = i % 3 != 0;
            let contents: i32 = if i % 7 == 0 { 0x4000_0001 } else { 1 };
            if is_axial {
                brushes.extend_from_slice(&contents.to_le_bytes());
                brushes.extend_from_slice(&(-1i32).to_le_bytes());
                brushes.extend_from_slice(&axial_i.to_le_bytes());
                axial_i += 1;
                axial += 1;
            } else {
                let n = 7 + (i % 4) as i32;
                brushes.extend_from_slice(&contents.to_le_bytes());
                brushes.extend_from_slice(&n.to_le_bytes());
                brushes.extend_from_slice(&first.to_le_bytes());
                first += n;
                solid_sides += n as usize;
            }
        }
        data.extend_from_slice(&brushes);
        while !(base + data.len() as u64).is_multiple_of(16) {
            data.push(0);
        }
        let mut fixed = Vec::new();
        for s in 0..solid_sides {
            let mut rec = [0u8; 16];
            let plane = (s * 7) % planes_n;
            rec[0..8].copy_from_slice(&(planes_addr + (plane * PLANE_RECORD) as u64).to_le_bytes());
            rec[8..10].copy_from_slice(&((s % 5) as u16).to_le_bytes());
            rec[10..12].copy_from_slice(&((s % 2) as u16).to_le_bytes());
            fixed.extend_from_slice(&rec);
        }
        data.extend_from_slice(&fixed);
        while !(base + data.len() as u64).is_multiple_of(16) {
            data.push(0);
        }
        for k in 0..axial {
            let mut rec = [0u8; 48];
            let lo = [k as f32, 0.0, 0.0];
            let hi = [k as f32 + 8.0, 16.0, 32.0];
            for a in 0..3 {
                rec[a * 4..a * 4 + 4].copy_from_slice(&lo[a].to_le_bytes());
                rec[16 + a * 4..20 + a * 4].copy_from_slice(&hi[a].to_le_bytes());
            }
            for s in 0..6 {
                rec[32 + s * 2..34 + s * 2].copy_from_slice(&((s % 3) as u16).to_le_bytes());
            }
            data.extend_from_slice(&rec);
        }
        let mut names = Vec::new();
        let mut name_addrs = Vec::new();
        let names_base = base + 0x10_0000;
        for n in ["tools/toolsnodraw", "brick/wall01", "tools/toolsskybox"] {
            name_addrs.push(names_base + names.len() as u64);
            names.extend_from_slice(n.as_bytes());
            names.push(0);
        }
        for i in 0..40 {
            let n = format!("concrete/floor{i:03}");
            name_addrs.push(names_base + names.len() as u64);
            names.extend_from_slice(n.as_bytes());
            names.push(0);
        }
        let mut surfaces = Vec::new();
        for a in &name_addrs {
            let mut rec = [0u8; 16];
            rec[0..4].copy_from_slice(&(*a as u32).to_le_bytes());
            surfaces.extend_from_slice(&rec);
        }
        let mut mem = VecMem::default();
        mem.add(base, data);
        mem.add(names_base, names);
        mem.add(base + 0x20_0000, surfaces);
        Layout { mem, brush_addr, axial, solid_sides, brushes: count }
    }

    #[test]
    fn brush_arrays_are_found_by_their_index_chains() {
        let l = layout();
        let hits = find_brush_arrays(&l.mem, &ScanOptions::default());
        assert!(!hits.is_empty());
        let (addr, brushes) = read_brush_array(&l.mem, hits[0]).expect("array");
        assert_eq!(addr, l.brush_addr);
        assert_eq!(brushes.len(), l.brushes);
        assert_eq!(brushes.iter().filter(|b| b.axial()).count(), l.axial);
    }

    #[test]
    fn surface_names_are_found_when_the_pointers_are_not_in_order() {
        let mut mem = VecMem::default();
        let names: Vec<String> = (0..12).map(|i| format!("mat/name{i:02}")).collect();
        let mut blob = Vec::new();
        let mut at = Vec::new();
        for n in &names {
            at.push(blob.len() as u64);
            blob.extend_from_slice(n.as_bytes());
            blob.push(0);
        }
        let blob_base = 0x2000_0000u64;
        let order = [5usize, 0, 9, 3, 11, 1, 7, 2, 10, 4, 8, 6];
        let mut array = Vec::new();
        for &k in &order {
            let mut rec = vec![0u8; SURFACE_RECORD];
            rec[0..8].copy_from_slice(&(blob_base + at[k]).to_le_bytes());
            array.extend(rec);
        }
        array.extend(vec![0xffu8; 400]);
        mem.add(blob_base, blob);
        mem.add(0x3000_0000, array);
        let (addr, found) = find_surface_array(&mem, &ScanOptions::default(), 12, None).expect("surfaces");
        assert_eq!(addr, 0x3000_0000);
        let want: Vec<String> = order.iter().map(|&k| names[k].clone()).collect();
        assert_eq!(found, want);
    }

    #[test]
    fn random_data_is_no_brush_array() {
        let mut m = VecMem::default();
        let mut x = 12345u32;
        let data: Vec<u8> = (0..400_000)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                (x >> 3) as u8
            })
            .collect();
        m.add(0x1000_0000, data);
        m.add(0x3000_0000, vec![0u8; 100_000]);
        assert!(find_brush_arrays(&m, &ScanOptions::default()).is_empty());
    }

    #[test]
    fn a_broken_chain_ends_the_run() {
        let mut recs = Vec::new();
        let mut first = 0;
        for i in 0..200 {
            if i % 2 == 0 {
                recs.push(BrushRec { contents: 1, numsides: -1, index: i / 2 });
            } else {
                recs.push(BrushRec { contents: 1, numsides: 8, index: first });
                first += 8;
            }
        }
        assert!(chain_window(&recs[..64]));
        recs[120].index += 3;
        assert_eq!(longest_run_around(&recs, 50), (0, 120));
        assert_eq!(longest_run_around(&recs, 150).0, 120.max(longest_run_around(&recs, 150).0));
        recs[10].numsides = 3;
        assert!(!chain_window(&recs[..64]));
    }

    #[test]
    fn the_whole_collision_set_is_decoded() {
        let l = layout();
        let cm = find_cm(&l.mem, &ScanOptions::default()).expect("cm");
        assert_eq!(cm.brushes.len(), l.brushes);
        assert_eq!(cm.planes, 800);
        assert_eq!(cm.sides.len(), l.solid_sides);
        assert_eq!(cm.boxes.len(), l.axial);
        assert_eq!(cm.total_sides(), l.solid_sides + l.axial * 6);
        assert_eq!(cm.boxes[3].mins, [3.0, 0.0, 0.0]);
        assert_eq!(cm.boxes[3].maxs, [11.0, 16.0, 32.0]);
        assert_eq!(cm.surface_names.len(), 43);
        assert_eq!(cm.surface_names[0], "tools/toolsnodraw");
        let k = cm.side_kinds();
        assert!(k.nodraw > 0 && k.sky > 0 && k.other > 0);
        assert_eq!(k.nodraw + k.sky + k.other + k.clip + k.tools + k.trigger, cm.total_sides());
    }

    #[test]
    fn plane_validation_follows_the_engine_rules() {
        assert!(valid_plane(&plane_bytes([1.0, 0.0, 0.0], 5.0, 0)));
        assert!(valid_plane(&plane_bytes([0.0, 0.0, -1.0], 5.0, 2)));
        assert!(!valid_plane(&plane_bytes([1.0, 0.0, 0.0], 5.0, 1)));
        assert!(!valid_plane(&plane_bytes([0.5, 0.0, 0.0], 5.0, 0)));
        assert!(!valid_plane(&plane_bytes([1.0, 0.0, 0.0], f32::NAN, 0)));
        let s = 0.6f32;
        let c = 0.8f32;
        assert!(valid_plane(&plane_bytes([c, s, 0.0], 5.0, 3)));
        assert!(valid_plane(&plane_bytes([s, c, 0.0], 5.0, 4)));
        assert!(!valid_plane(&plane_bytes([c, s, 0.0], 5.0, 4)));
        assert!(!valid_plane(&[0u8; 20]));
        assert!(!valid_plane(&[0u8; 8]));
    }
}
