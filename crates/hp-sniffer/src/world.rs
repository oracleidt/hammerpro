use rayon::prelude::*;

use crate::cm::{PLANE_RECORD, window_around};
use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

pub const FACE_RECORD: usize = 40;
pub const TEXINFO_RECORD: usize = 88;
const WINDOW: usize = 64;
pub const MATERIAL_ID_AT: u64 = 0x2c;
const MIN_TEXINFO_RUN: usize = 8;
pub const FACE_HAS_DISP: u16 = 0x800;
const PLANE_FIT_TOLERANCE: f32 = 0.1;
const QUICK_TOLERANCE: f32 = 0.5;
const VI_PAD_MAX: u64 = 64;
const WIDE_REACH: u64 = 6 << 20;
const WIDE_TESTS: usize = 8;
const VI_SWEEP: u64 = 16384;
const VERTEX_SLACK: u64 = 2048;
const MAX_VERTS: usize = 400_000;

fn vbytes_of(entries: usize) -> u64 {
    (entries * 2) as u64
}
const MAX_FIRST_STEP: i32 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RenderFace {
    pub plane: u32,
    pub back: bool,
    pub first: i32,
    pub count: u8,
    pub texinfo: u16,
    pub disp: u64,
    pub has_disp: bool,
    pub material_sort: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderTexInfo {
    pub vecs: [f32; 16],
    pub flags: i32,
    pub material: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderWorld {
    pub faces_addr: u64,
    pub faces: Vec<RenderFace>,
    pub texinfo_addr: u64,
    pub texinfo: Vec<RenderTexInfo>,
    pub vertindices_addr: u64,
    pub vertindices: Vec<u16>,
    pub vertexes_addr: u64,
    pub vertexes: Vec<[f32; 3]>,
    pub material_ids: std::collections::HashMap<u64, u32>,
    pub texinfo_ranks: Vec<i32>,
    pub texinfo_file_ranks: Vec<i32>,
}

fn u16_at(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn u64_at(d: &[u8], at: usize) -> u64 {
    u64::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3], d[at + 4], d[at + 5], d[at + 6], d[at + 7]])
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

pub fn parse_face(rec: &[u8], planes_base: u64, planes: usize) -> Option<RenderFace> {
    let ptr = u64_at(rec, 8);
    if ptr < planes_base {
        return None;
    }
    let off = ptr - planes_base;
    if !off.is_multiple_of(PLANE_RECORD as u64) || off / PLANE_RECORD as u64 >= planes as u64 {
        return None;
    }
    let count = rec[7];
    if !(3..=64).contains(&count) {
        return None;
    }
    let first = i32_at(rec, 16);
    if first < 0 {
        return None;
    }
    Some(RenderFace {
        plane: (off / PLANE_RECORD as u64) as u32,
        back: u16_at(rec, 4) & 0x40 != 0,
        first,
        count,
        texinfo: u16_at(rec, 30) >> 1,
        disp: u64_at(rec, 32),
        has_disp: u16_at(rec, 4) & FACE_HAS_DISP != 0,
        material_sort: u16_at(rec, 26),
    })
}

fn face_run_ok(window: &[Option<RenderFace>]) -> bool {
    let mut ranges: Vec<(i32, i32)> = Vec::with_capacity(window.len());
    for f in window {
        let Some(f) = f else { return false };
        ranges.push((f.first, f.first + i32::from(f.count)));
    }
    ranges.sort_unstable();
    ranges.windows(2).all(|w| w[1].0 >= w[0].1 && w[1].0 - w[0].1 <= MAX_FIRST_STEP)
}

pub fn parse_faces(data: &[u8], planes_base: u64, planes: usize) -> Vec<Option<RenderFace>> {
    data.chunks_exact(FACE_RECORD).map(|r| parse_face(r, planes_base, planes)).collect()
}

pub fn face_run(faces: &[Option<RenderFace>], at: usize) -> (usize, usize) {
    let mut start = at;
    while start > 0 && faces[start - 1].is_some() {
        start -= 1;
    }
    let mut end = at + 1;
    while end < faces.len() && faces[end].is_some() {
        end += 1;
    }
    (start, end)
}

pub fn find_faces<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    planes_addr: u64,
    planes: usize,
) -> Option<(u64, Vec<RenderFace>)> {
    for window in [WINDOW, 32, 16, 8] {
        if let Some(found) = find_faces_w(src, opts, planes_addr, planes, window) {
            return Some(found);
        }
    }
    None
}

pub fn find_faces_w<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    planes_addr: u64,
    planes: usize,
    window_len: usize,
) -> Option<(u64, Vec<RenderFace>)> {
    let hi = planes_addr + (planes * PLANE_RECORD) as u64;
    let opts = ScanOptions { overlap: FACE_RECORD * (window_len + 2), writable_only: true, ..*opts };
    let mut hits: Vec<u64> = par_chunks(src, &opts, |base, data, own| {
        let mut out = Vec::new();
        let limit = own.min(data.len());
        let mut j = 0usize;
        while j + 8 <= limit && j + FACE_RECORD * window_len <= data.len() {
            let ptr = u64_at(data, j);
            if ptr >= planes_addr && ptr < hi && j >= 8 {
                let start = j - 8;
                if start + FACE_RECORD * window_len <= data.len() {
                    let window: Vec<Option<RenderFace>> =
                        parse_faces(&data[start..start + FACE_RECORD * window_len], planes_addr, planes);
                    if face_run_ok(&window) {
                        out.push(base + start as u64);
                        j = start + FACE_RECORD * window_len;
                        continue;
                    }
                }
            }
            j += 4;
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    hits.sort_unstable();
    let mut best: Option<(u64, Vec<RenderFace>)> = None;
    let mut covered: Vec<(u64, u64)> = Vec::new();
    for hit in hits {
        if covered.iter().any(|&(lo, hi)| hit >= lo && hit < hi) {
            continue;
        }
        let reach = 2_000_000u64;
        let Some((from, data)) =
            window_around(src, hit, reach * FACE_RECORD as u64 / 4, reach * FACE_RECORD as u64 / 4)
        else {
            continue;
        };
        let skew = ((hit - from) % FACE_RECORD as u64) as usize;
        let from = from + skew as u64;
        let data = &data[skew.min(data.len())..];
        let parsed = parse_faces(data, planes_addr, planes);
        let at = ((hit - from) / FACE_RECORD as u64) as usize;
        if at >= parsed.len() {
            continue;
        }
        let (s, e) = face_run(&parsed, at);
        covered.push((from + (s * FACE_RECORD) as u64, from + (e * FACE_RECORD) as u64));
        if e - s < window_len || best.as_ref().is_some_and(|b| b.1.len() >= e - s) {
            continue;
        }
        let faces: Vec<RenderFace> = parsed[s..e].iter().filter_map(|f| *f).collect();
        best = Some((from + (s * FACE_RECORD) as u64, faces));
    }
    best
}

pub fn parse_texinfo(rec: &[u8]) -> Option<RenderTexInfo> {
    let mut vecs = [0f32; 16];
    for (k, v) in vecs.iter_mut().enumerate() {
        *v = f32_at(rec, k * 4);
        if !v.is_finite() || v.abs() > 1.0e9 {
            return None;
        }
    }
    let ptr = u64_at(rec, 80);
    if ptr < 0x10000 || ptr >> 32 != 0 {
        return None;
    }
    if i32_at(rec, 76) != 0 {
        return None;
    }
    Some(RenderTexInfo { vecs, flags: i32_at(rec, 72), material: ptr })
}

pub fn find_texinfo<S: MemSource + ?Sized>(src: &S, before: u64, wanted: usize) -> Option<(u64, Vec<RenderTexInfo>)> {
    find_texinfo_fit(src, before, wanted, &[], &[])
}

fn texinfo_runs(data: &[u8], base: u64, wanted: usize) -> Vec<(u64, usize)> {
    let mut out: Vec<(u64, usize)> = Vec::new();
    for phase in 0..(TEXINFO_RECORD / 4) {
        let off = phase * 4;
        let recs: Vec<bool> =
            data.get(off..).unwrap_or(&[]).chunks_exact(TEXINFO_RECORD).map(|c| parse_texinfo(c).is_some()).collect();
        let mut i = 0usize;
        while i < recs.len() {
            if !recs[i] {
                i += 1;
                continue;
            }
            let start = i;
            while i < recs.len() && recs[i] {
                i += 1;
            }
            let len = i - start;
            if len >= MIN_TEXINFO_RUN && len >= wanted {
                out.push((base + (off + start * TEXINFO_RECORD) as u64, len));
            }
        }
    }
    out
}

fn texinfo_axes_fit(list: &[RenderTexInfo], faces: &[RenderFace], planes: &[[f32; 4]]) -> f64 {
    let mut ok = 0usize;
    let mut total = 0usize;
    let step = (faces.len() / 600).max(1);
    for f in faces.iter().step_by(step) {
        let (Some(t), Some(p)) = (list.get(usize::from(f.texinfo)), planes.get(f.plane as usize)) else {
            total += 1;
            continue;
        };
        total += 1;
        let dot = |v: &[f32]| {
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if len < 1e-6 { 0.0 } else { (v[0] * p[0] + v[1] * p[1] + v[2] * p[2]).abs() / len }
        };
        if dot(&t.vecs[0..3]) < 0.02 && dot(&t.vecs[4..7]) < 0.02 {
            ok += 1;
        }
    }
    if total == 0 { 0.0 } else { ok as f64 / total as f64 }
}

pub fn find_texinfo_fit<S: MemSource + ?Sized>(
    src: &S,
    before: u64,
    wanted: usize,
    faces: &[RenderFace],
    planes: &[[f32; 4]],
) -> Option<(u64, Vec<RenderTexInfo>)> {
    let near = (wanted.max(1024) * TEXINFO_RECORD * 3) as u64;
    let far = 6u64 << 20;
    let mut cands: Vec<(u64, usize)> = Vec::new();
    for span in [near, far] {
        let (from, data) = window_around(src, before.saturating_sub(1), span, 16)?;
        let base = from - from % 4 + if from % 4 == 0 { 0 } else { 4 };
        let data = &data[(base - from) as usize..];
        cands = texinfo_runs(data, base, wanted);
        if !cands.is_empty() && (faces.is_empty() || span == far || cands.len() == 1) {
            break;
        }
    }
    let mut scored: Vec<(f64, u64, usize)> = Vec::new();
    for (addr, len) in cands {
        let raw = src.read_vec(addr, len * TEXINFO_RECORD);
        let list: Vec<RenderTexInfo> = raw.chunks_exact(TEXINFO_RECORD).filter_map(parse_texinfo).collect();
        if list.len() != len {
            continue;
        }
        let fit = if faces.is_empty() { 1.0 } else { texinfo_axes_fit(&list, faces, planes) };
        scored.push((fit, addr, len));
    }
    let top = scored.iter().map(|s| s.0).fold(0.0f64, f64::max);
    let (_, addr, len) =
        scored.iter().filter(|s| s.0 >= top - 0.02).max_by(|a, b| a.2.cmp(&b.2).then(a.1.cmp(&b.1))).copied()?;
    let raw = src.read_vec(addr, len * TEXINFO_RECORD);
    let list: Vec<RenderTexInfo> = raw.chunks_exact(TEXINFO_RECORD).filter_map(parse_texinfo).collect();
    (list.len() == len).then_some((addr, list))
}

pub fn read_u16s<S: MemSource + ?Sized>(src: &S, addr: u64, count: usize) -> Option<Vec<u16>> {
    let raw = src.read_vec(addr, count * 2);
    if raw.len() != count * 2 {
        return None;
    }
    Some(raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect())
}

pub fn read_vecs<S: MemSource + ?Sized>(src: &S, addr: u64, count: usize) -> Option<Vec<[f32; 3]>> {
    let raw = src.read_vec(addr, count * 12);
    if raw.len() != count * 12 {
        return None;
    }
    Some(raw.chunks_exact(12).map(|c| [f32_at(c, 0), f32_at(c, 4), f32_at(c, 8)]).collect())
}

pub fn plane_fit(faces: &[RenderFace], planes: &[[f32; 4]], vi: &[u16], verts: &[[f32; 3]], sample: usize) -> f64 {
    let step = (faces.len() / sample.max(1)).max(1);
    let mut ok = 0usize;
    let mut total = 0usize;
    for f in faces.iter().step_by(step) {
        let Some(p) = planes.get(f.plane as usize) else { continue };
        let first = f.first as usize;
        let n = usize::from(f.count);
        let Some(idx) = vi.get(first..first + n) else { continue };
        total += 1;
        let fits = idx.iter().all(|&i| {
            verts.get(usize::from(i)).is_some_and(|v| {
                let d = v[0] * p[0] + v[1] * p[1] + v[2] * p[2] - p[3];
                d.abs() < PLANE_FIT_TOLERANCE
            })
        });
        if fits {
            ok += 1;
        }
    }
    if total == 0 { 0.0 } else { ok as f64 / total as f64 }
}

fn test_faces(faces: &[RenderFace], planes: &[[f32; 4]], limit: usize) -> Vec<([f32; 4], usize, usize)> {
    let mut tests = Vec::new();
    let mut used: std::collections::HashSet<u32> = std::collections::HashSet::new();
    for f in faces.iter().step_by((faces.len() / 64).max(1)) {
        if tests.len() >= limit {
            break;
        }
        let Some(p) = planes.get(f.plane as usize) else { continue };
        if used.insert(f.plane) {
            tests.push((*p, f.first as usize, usize::from(f.count)));
        }
    }
    tests
}

type Scored = (f64, u64, Vec<u16>, u64, Vec<[f32; 3]>);

pub fn read_material_ids<S: MemSource + ?Sized>(
    src: &S,
    texinfo: &[RenderTexInfo],
) -> std::collections::HashMap<u64, u32> {
    let mut ptrs: Vec<u64> = texinfo.iter().map(|t| t.material).collect();
    ptrs.sort_unstable();
    ptrs.dedup();
    let mut out = std::collections::HashMap::new();
    for p in ptrs {
        let raw = src.read_vec(p + MATERIAL_ID_AT, 4);
        if raw.len() == 4 {
            out.insert(p, u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]));
        }
    }
    let unique: std::collections::HashSet<u32> = out.values().copied().collect();
    if unique.len() != out.len() {
        out.clear();
    }
    out
}

pub fn find_render_world<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    planes_addr: u64,
    plane_bytes: &[u8],
) -> Option<RenderWorld> {
    let planes = plane_bytes.len() / PLANE_RECORD;
    let plane_vals: Vec<[f32; 4]> = plane_bytes
        .chunks_exact(PLANE_RECORD)
        .map(|c| [f32_at(c, 0), f32_at(c, 4), f32_at(c, 8), f32_at(c, 12)])
        .collect();
    let (faces_addr, faces) = find_faces(src, opts, planes_addr, planes)?;
    let need_vi = faces.iter().map(|f| f.first as usize + usize::from(f.count)).max()?;
    let max_texinfo = faces.iter().map(|f| usize::from(f.texinfo)).max()? + 1;
    let (texinfo_addr, texinfo) = find_texinfo_fit(src, faces_addr, max_texinfo, &faces, &plane_vals)?;
    let mut best: Option<Scored> = None;
    let vi_bytes = (need_vi * 2) as u64;
    let texinfo_end = texinfo_addr + (texinfo.len() * TEXINFO_RECORD) as u64;
    let faces_end = faces_addr + (faces.len() * FACE_RECORD) as u64;
    let mut candidates: Vec<u64> = Vec::new();
    for pad in (0..=VI_PAD_MAX).step_by(2) {
        candidates.push(texinfo_addr.saturating_sub(vi_bytes + pad));
        candidates.push(texinfo_end + pad);
        candidates.push(faces_addr.saturating_sub(vi_bytes + pad));
        candidates.push(faces_end + pad);
    }
    for anchor in [texinfo_addr, faces_addr] {
        let base = anchor.saturating_sub(vi_bytes) & !15;
        for k in 0..=VI_SWEEP {
            candidates.push(base.saturating_sub(16 * k));
        }
    }
    for anchor in [texinfo_end, faces_end] {
        let base = (anchor + 15) & !15;
        for k in 0..=VI_SWEEP {
            candidates.push(base + 16 * k);
        }
    }
    candidates.sort_unstable();
    candidates.dedup();
    let big = window_around(src, texinfo_addr, WIDE_REACH, WIDE_REACH);
    let tests = test_faces(&faces, &plane_vals, WIDE_TESTS);
    let scored: Vec<Scored> = candidates
        .par_iter()
        .filter_map(|&vi_addr| {
            let vi = match &big {
                Some((lo, buf)) if vi_addr >= *lo && vi_addr + vi_bytes <= *lo + buf.len() as u64 => {
                    let at = (vi_addr - lo) as usize;
                    buf[at..at + need_vi * 2].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect()
                }
                _ => read_u16s(src, vi_addr, need_vi)?,
            };
            let nverts = faces
                .iter()
                .flat_map(|f| vi[f.first as usize..f.first as usize + usize::from(f.count)].iter())
                .map(|&i| usize::from(i) + 1)
                .max()?;
            if !(4..=MAX_VERTS).contains(&nverts) {
                return None;
            }
            let vbytes = (nverts * 12) as u64;
            let vi_end = vi_addr + vbytes_of(need_vi);
            let (lo, window) = match &big {
                Some((blo, buf))
                    if vi_addr >= blo + vbytes + VERTEX_SLACK
                        && vi_end + VERTEX_SLACK + vbytes <= blo + buf.len() as u64 =>
                {
                    (*blo, std::borrow::Cow::Borrowed(buf.as_slice()))
                }
                _ => {
                    let (l, w) =
                        window_around(src, vi_addr, vbytes + VERTEX_SLACK, vbytes_of(need_vi) + VERTEX_SLACK + vbytes)?;
                    (l, std::borrow::Cow::Owned(w))
                }
            };
            if window.len() < nverts * 12 {
                return None;
            }
            let mut best: Option<(f64, u64, Vec<[f32; 3]>)> = None;
            let mut starts: Vec<u64> = Vec::new();
            for delta in (0..=VERTEX_SLACK).step_by(4) {
                starts.push(vi_addr.saturating_sub(vbytes + delta));
                starts.push(vi_end + delta);
            }
            starts.sort_unstable();
            starts.dedup();
            for v_addr in starts {
                if v_addr < lo || v_addr + vbytes > lo + window.len() as u64 {
                    continue;
                }
                let at = (v_addr - lo) as usize;
                if !tests.iter().all(|(p, first, count)| {
                    vi[*first..first + count].iter().all(|&i| {
                        let o = at + usize::from(i) * 12;
                        let v = [f32_at(&window, o), f32_at(&window, o + 4), f32_at(&window, o + 8)];
                        (v[0] * p[0] + v[1] * p[1] + v[2] * p[2] - p[3]).abs() < QUICK_TOLERANCE
                    })
                }) {
                    continue;
                }
                let verts: Vec<[f32; 3]> = window[at..at + nverts * 12]
                    .chunks_exact(12)
                    .map(|c| [f32_at(c, 0), f32_at(c, 4), f32_at(c, 8)])
                    .collect();
                if verts.iter().any(|v| v.iter().any(|x| !x.is_finite() || x.abs() > 1.0e6)) {
                    continue;
                }
                let score = plane_fit(&faces, &plane_vals, &vi, &verts, 400);
                if best.as_ref().is_none_or(|b| score > b.0) {
                    best = Some((score, v_addr, verts));
                }
            }
            best.map(|(score, v_addr, verts)| (score, vi_addr, vi, v_addr, verts))
        })
        .collect();
    for s in scored {
        if best.as_ref().is_none_or(|b| s.0 > b.0) {
            best = Some(s);
        }
    }
    let (score, vi_addr, vi, v_addr, verts) = best?;
    if score < 0.98 {
        return None;
    }
    let material_ids = read_material_ids(src, &texinfo);
    Some(RenderWorld {
        faces_addr,
        faces,
        texinfo_addr,
        texinfo,
        vertindices_addr: vi_addr,
        vertindices: vi,
        vertexes_addr: v_addr,
        vertexes: verts,
        material_ids,
        texinfo_ranks: Vec::new(),
        texinfo_file_ranks: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texinfo_bytes(seed: f32) -> Vec<u8> {
        let mut rec = vec![0u8; TEXINFO_RECORD];
        for k in 0..16 {
            rec[k * 4..k * 4 + 4].copy_from_slice(&(seed + k as f32).to_le_bytes());
        }
        rec[80..88].copy_from_slice(&0x3ec34f4u64.to_le_bytes());
        rec
    }

    #[test]
    fn texinfo_is_found_far_before_the_faces() {
        let mut mem = crate::procmem::VecMem::default();
        let mut data = Vec::new();
        for i in 0..100 {
            data.extend(texinfo_bytes(i as f32));
        }
        let texinfo_len = data.len();
        data.extend(vec![0xffu8; 400_000]);
        let before = 0x2000_0000u64 + data.len() as u64;
        mem.add(0x2000_0000, data);
        let (addr, list) = find_texinfo(&mem, before, 90).expect("texinfo");
        assert_eq!((addr, list.len()), (0x2000_0000, 100));
        assert!(texinfo_len < 400_000);
    }

    #[test]
    fn texinfo_axes_must_lie_in_the_face_planes() {
        let mut list = Vec::new();
        for i in 0..4 {
            let mut vecs = [0f32; 16];
            vecs[0] = 1.0;
            vecs[5] = 1.0;
            list.push(RenderTexInfo { vecs, flags: i, material: 0x1000 });
        }
        let face = |texinfo| RenderFace {
            plane: 0,
            back: false,
            first: 0,
            count: 3,
            texinfo,
            disp: 0,
            has_disp: false,
            material_sort: 0,
        };
        let faces = vec![face(0), face(1), face(2)];
        assert_eq!(texinfo_axes_fit(&list, &faces, &[[0.0, 0.0, 1.0, 0.0]]), 1.0);
        assert_eq!(texinfo_axes_fit(&list, &faces, &[[1.0, 0.0, 0.0, 0.0]]), 0.0);
    }

    fn face_bytes(plane_ptr: u64, first: i32, count: u8, texinfo: u16, back: bool, disp: u64) -> Vec<u8> {
        let mut r = vec![0u8; FACE_RECORD];
        let bits: u16 = if back { 0x40 } else { 0 } | if disp != 0 { FACE_HAS_DISP } else { 0 };
        r[4..6].copy_from_slice(&bits.to_le_bytes());
        r[7] = count;
        r[8..16].copy_from_slice(&plane_ptr.to_le_bytes());
        r[16..20].copy_from_slice(&first.to_le_bytes());
        r[30..32].copy_from_slice(&((texinfo << 1) | 1).to_le_bytes());
        r[32..40].copy_from_slice(&disp.to_le_bytes());
        r
    }

    #[test]
    fn a_face_record_is_decoded() {
        let base = 0x1000_0000u64;
        let rec = face_bytes(base + 20 * 7, 40, 4, 1234, true, 0);
        let f = parse_face(&rec, base, 100).expect("face");
        assert_eq!((f.plane, f.back, f.first, f.count, f.texinfo, f.disp), (7, true, 40, 4, 1234, 0));
        let disp = face_bytes(base, 0, 4, 5, false, 0xdead0000);
        assert_eq!(parse_face(&disp, base, 100).map(|f| f.disp), Some(0xdead0000));
        assert!(parse_face(&face_bytes(base + 3, 0, 4, 1, false, 0), base, 100).is_none(), "misaligned plane");
        assert!(parse_face(&face_bytes(base + 20 * 100, 0, 4, 1, false, 0), base, 100).is_none(), "plane out of range");
        assert!(parse_face(&face_bytes(base, 0, 2, 1, false, 0), base, 100).is_none(), "too few vertices");
        assert!(parse_face(&face_bytes(base, -4, 4, 1, false, 0), base, 100).is_none());
    }

    #[test]
    fn face_runs_follow_the_vertex_index_steps() {
        let base = 0x1000_0000u64;
        let mut data = Vec::new();
        let mut first = 0i32;
        for i in 0..200 {
            data.extend(face_bytes(base + 20 * (i % 50), first, 4, (i % 9) as u16, i % 2 == 0, 0));
            first += 8;
        }
        let faces = parse_faces(&data, base, 50);
        assert_eq!(face_run(&faces, 100), (0, 200));
        let mut broken = data.clone();
        broken[FACE_RECORD * 120 + 8..FACE_RECORD * 120 + 16].copy_from_slice(&1u64.to_le_bytes());
        let faces = parse_faces(&broken, base, 50);
        assert_eq!(face_run(&faces, 10), (0, 120));
        assert!(face_run_ok(&faces[..64]));
        assert!(!face_run_ok(&faces[100..170]));
        let mut shuffled: Vec<Option<RenderFace>> = parse_faces(&data, base, 50);
        shuffled[..64].reverse();
        assert!(face_run_ok(&shuffled[..64]), "a permuted window still has disjoint vertex index ranges");
        let mut overlapping = parse_faces(&data, base, 50);
        overlapping[5].as_mut().expect("face").first = overlapping[4].expect("face").first;
        assert!(!face_run_ok(&overlapping[..64]));
    }

    #[test]
    fn texinfo_records_need_a_material_pointer() {
        let mut rec = vec![0u8; TEXINFO_RECORD];
        for k in 0..16 {
            rec[k * 4..k * 4 + 4].copy_from_slice(&(k as f32).to_le_bytes());
        }
        rec[72..76].copy_from_slice(&2048i32.to_le_bytes());
        rec[80..88].copy_from_slice(&0x3ec34f4u64.to_le_bytes());
        let t = parse_texinfo(&rec).expect("texinfo");
        assert_eq!((t.flags, t.material, t.vecs[5]), (2048, 0x3ec34f4, 5.0));
        let mut high = rec.clone();
        high[84] = 1;
        assert!(parse_texinfo(&high).is_none());
        let mut nan = rec.clone();
        nan[0..4].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(parse_texinfo(&nan).is_none());
        assert!(parse_texinfo(&[0u8; TEXINFO_RECORD]).is_none());
    }

    #[test]
    fn plane_fit_scores_the_right_arrays_higher() {
        let planes = vec![[0.0, 0.0, 1.0, 8.0]];
        let faces = vec![RenderFace {
            plane: 0,
            back: false,
            first: 0,
            count: 3,
            texinfo: 0,
            disp: 0,
            has_disp: false,
            material_sort: 0,
        }];
        let good = vec![[0.0, 0.0, 8.0], [10.0, 0.0, 8.0], [0.0, 10.0, 8.0]];
        let bad = vec![[0.0, 0.0, 9.0], [10.0, 0.0, 8.0], [0.0, 10.0, 8.0]];
        assert_eq!(plane_fit(&faces, &planes, &[0, 1, 2], &good, 10), 1.0);
        assert_eq!(plane_fit(&faces, &planes, &[0, 1, 2], &bad, 10), 0.0);
    }
}
