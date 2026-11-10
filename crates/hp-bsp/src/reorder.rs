use std::collections::HashMap;

use crate::header::lump;
use crate::rng::Rng;
use crate::{Bsp, Result};

const PLANES: usize = 1;
const TEXDATA: usize = lump::TEXDATA;
const VERTEXES: usize = 3;
const NODES: usize = 5;
const TEXINFO: usize = 6;
const OCCLUSION: usize = 9;
const FACEIDS: usize = 11;
const EDGES: usize = 12;
const SURFEDGES: usize = 13;
const MODELS: usize = lump::MODELS;
const LEAFFACES: usize = 16;
const LEAFBRUSHES: usize = 17;
const BRUSHES: usize = 18;
const BRUSHSIDES: usize = 19;
const AREAPORTALS: usize = 21;
const DISPINFO: usize = 26;
const LEAFWATERDATA: usize = 36;
const VERTNORMALINDICES: usize = 31;
const FACE_MACRO: usize = 47;
const WATEROVERLAYS: usize = 50;
const FACES_HDR: usize = lump::FACES_HDR;
const FACE_LUMPS: [usize; 3] = [lump::FACES, lump::ORIGINAL_FACES, FACES_HDR];

const PLANE_LEN: usize = 20;
const VERTEX_LEN: usize = 12;
const EDGE_LEN: usize = 4;
const TEXINFO_LEN: usize = 72;
const TEXDATA_LEN: usize = 32;
const NODE_LEN: usize = 32;
const FACE_LEN: usize = 56;
const BRUSH_LEN: usize = 12;
const SIDE_LEN: usize = 8;
const MODEL_LEN: usize = 48;
const DISP_LEN: usize = 176;
const OVERLAY_LEN: usize = 352;
const WATER_OVERLAY_LEN: usize = 1120;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReorderOptions {
    pub tables: bool,
    pub world: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReorderReport {
    pub applied: Vec<&'static str>,
    pub skipped: Vec<&'static str>,
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn i32_at(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

fn put32(b: &mut [u8], at: usize, v: i32) {
    b[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

fn count(data: &[u8], stride: usize) -> Option<usize> {
    (data.len().is_multiple_of(stride)).then_some(data.len() / stride)
}

struct Work<'a> {
    bsp: &'a Bsp,
    cur: HashMap<usize, Vec<u8>>,
}

impl<'a> Work<'a> {
    fn get(&mut self, index: usize) -> Result<Vec<u8>> {
        if let Some(v) = self.cur.get(&index) {
            return Ok(v.clone());
        }
        Ok(self.bsp.lump(index)?.into_owned())
    }

    fn set(&mut self, index: usize, data: Vec<u8>) {
        self.cur.insert(index, data);
    }
}

fn permute_records(data: &[u8], stride: usize, new_to_old: &[usize]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for &old in new_to_old {
        out.extend_from_slice(&data[old * stride..(old + 1) * stride]);
    }
    out
}

fn inverse(new_to_old: &[usize]) -> Vec<usize> {
    let mut map = vec![0usize; new_to_old.len()];
    for (new, &old) in new_to_old.iter().enumerate() {
        map[old] = new;
    }
    map
}

fn shuffled_identity(n: usize, fixed_prefix: usize, rng: &mut Rng) -> Vec<usize> {
    let mut v: Vec<usize> = (0..n).collect();
    rng.shuffle(&mut v[fixed_prefix.min(n)..]);
    v
}

fn remap_u16_field(data: &mut [u8], stride: usize, at: usize, map: &[usize], signed: bool) -> bool {
    remap_u16(data, stride, at, map, signed, false)
}

fn remap_u16(data: &mut [u8], stride: usize, at: usize, map: &[usize], signed: bool, lenient: bool) -> bool {
    let n = data.len() / stride;
    for r in 0..n {
        let p = r * stride + at;
        let raw = u16_at(data, p);
        if signed && (raw as i16) < 0 {
            continue;
        }
        let Some(&m) = map.get(raw as usize) else {
            if lenient {
                continue;
            }
            return false;
        };
        put16(data, p, m as u16);
    }
    true
}

fn remap_i32_field(data: &mut [u8], stride: usize, at: usize, map: &[usize]) -> bool {
    let n = data.len() / stride;
    for r in 0..n {
        let p = r * stride + at;
        let v = i32_at(data, p);
        if v < 0 {
            continue;
        }
        let Some(&m) = map.get(v as usize) else { return false };
        put32(data, p, m as i32);
    }
    true
}

fn occlusion_plane_offsets(b: &[u8], version: u32) -> Option<Vec<usize>> {
    if b.is_empty() {
        return Some(Vec::new());
    }
    if b.len() < 4 {
        return None;
    }
    let n = usize::try_from(i32_at(b, 0)).ok()?;
    let stride = if version >= 2 { 40 } else { 36 };
    let mut pos = 4usize.checked_add(n.checked_mul(stride)?)?;
    if pos + 4 > b.len() {
        return None;
    }
    let polys = usize::try_from(i32_at(b, pos)).ok()?;
    pos += 4;
    let poly_start = pos;
    pos = pos.checked_add(polys.checked_mul(8)?)?;
    if pos + 4 > b.len() {
        return None;
    }
    let idx = usize::try_from(i32_at(b, pos)).ok()?;
    pos = pos.checked_add(4)?.checked_add(idx.checked_mul(4)?)?;
    if pos != b.len() {
        return None;
    }
    Some((0..polys).map(|p| poly_start + p * 8 + 4).collect())
}

fn plane_pair_order(n: usize, rng: &mut Rng) -> Vec<usize> {
    let groups = n / 2;
    let mut order: Vec<usize> = (0..groups).collect();
    rng.shuffle(&mut order);
    let mut out = Vec::with_capacity(n);
    for g in order {
        out.push(g * 2);
        out.push(g * 2 + 1);
    }
    if n % 2 == 1 {
        out.push(n - 1);
    }
    out
}

fn planes(w: &mut Work<'_>, rng: &mut Rng) -> Result<bool> {
    let data = w.get(PLANES)?;
    let Some(n) = count(&data, PLANE_LEN) else { return Ok(false) };
    if n < 4 {
        return Ok(false);
    }
    let new_to_old = plane_pair_order(n, rng);
    let map = inverse(&new_to_old);
    let mut nodes = w.get(NODES)?;
    let mut sides = w.get(BRUSHSIDES)?;
    let mut portals = w.get(AREAPORTALS)?;
    let mut occ = w.get(OCCLUSION)?;
    let mut faces: Vec<(usize, Vec<u8>)> = Vec::new();
    for l in FACE_LUMPS {
        faces.push((l, w.get(l)?));
    }
    if count(&nodes, NODE_LEN).is_none()
        || count(&sides, SIDE_LEN).is_none()
        || count(&portals, 12).is_none()
        || faces.iter().any(|(_, f)| count(f, FACE_LEN).is_none())
    {
        return Ok(false);
    }
    let occ_version = w.bsp.header().lumps[OCCLUSION].version;
    let Some(occ_at) = occlusion_plane_offsets(&occ, occ_version) else { return Ok(false) };
    let mut ok = remap_i32_field(&mut nodes, NODE_LEN, 0, &map)
        && remap_u16_field(&mut sides, SIDE_LEN, 0, &map, false)
        && remap_i32_field(&mut portals, 12, 8, &map);
    for (l, f) in faces.iter_mut() {
        ok &= remap_u16(f, FACE_LEN, 0, &map, false, *l == lump::ORIGINAL_FACES);
    }
    for at in occ_at {
        let v = i32_at(&occ, at);
        match usize::try_from(v).ok().and_then(|v| map.get(v)) {
            Some(&m) => put32(&mut occ, at, m as i32),
            None => ok = false,
        }
    }
    if !ok {
        return Ok(false);
    }
    w.set(PLANES, permute_records(&data, PLANE_LEN, &new_to_old));
    w.set(NODES, nodes);
    w.set(BRUSHSIDES, sides);
    w.set(AREAPORTALS, portals);
    w.set(OCCLUSION, occ);
    for (l, f) in faces {
        w.set(l, f);
    }
    Ok(true)
}

fn texinfo(w: &mut Work<'_>, rng: &mut Rng) -> Result<bool> {
    let data = w.get(TEXINFO)?;
    let Some(n) = count(&data, TEXINFO_LEN) else { return Ok(false) };
    if n < 3 {
        return Ok(false);
    }
    let new_to_old = shuffled_identity(n, 0, rng);
    let map = inverse(&new_to_old);
    let mut sides = w.get(BRUSHSIDES)?;
    let mut overlays = w.get(lump::OVERLAYS)?;
    let mut water_overlays = w.get(WATEROVERLAYS)?;
    let mut water = w.get(LEAFWATERDATA)?;
    let mut faces: Vec<(usize, Vec<u8>)> = Vec::new();
    for l in FACE_LUMPS {
        faces.push((l, w.get(l)?));
    }
    if count(&sides, SIDE_LEN).is_none()
        || count(&overlays, OVERLAY_LEN).is_none()
        || count(&water_overlays, WATER_OVERLAY_LEN).is_none()
        || count(&water, 12).is_none()
        || faces.iter().any(|(_, f)| count(f, FACE_LEN).is_none())
    {
        return Ok(false);
    }
    let mut ok = remap_u16_field(&mut sides, SIDE_LEN, 2, &map, true)
        && remap_u16_field(&mut overlays, OVERLAY_LEN, 4, &map, true)
        && remap_u16_field(&mut water_overlays, WATER_OVERLAY_LEN, 4, &map, true)
        && remap_i32_field(&mut water, 12, 8, &map);
    for (l, f) in faces.iter_mut() {
        ok &= remap_u16(f, FACE_LEN, 10, &map, true, *l == lump::ORIGINAL_FACES);
    }
    if !ok {
        return Ok(false);
    }
    w.set(TEXINFO, permute_records(&data, TEXINFO_LEN, &new_to_old));
    w.set(BRUSHSIDES, sides);
    w.set(lump::OVERLAYS, overlays);
    if !water_overlays.is_empty() {
        w.set(WATEROVERLAYS, water_overlays);
    }
    w.set(LEAFWATERDATA, water);
    for (l, f) in faces {
        w.set(l, f);
    }
    Ok(true)
}

fn texdata(w: &mut Work<'_>, rng: &mut Rng) -> Result<bool> {
    let data = w.get(TEXDATA)?;
    let Some(n) = count(&data, TEXDATA_LEN) else { return Ok(false) };
    let mut info = w.get(TEXINFO)?;
    if n < 3 || count(&info, TEXINFO_LEN).is_none() {
        return Ok(false);
    }
    let new_to_old = shuffled_identity(n, 0, rng);
    let map = inverse(&new_to_old);
    if !remap_i32_field(&mut info, TEXINFO_LEN, 68, &map) {
        return Ok(false);
    }
    w.set(TEXDATA, permute_records(&data, TEXDATA_LEN, &new_to_old));
    w.set(TEXINFO, info);
    Ok(true)
}

fn vertexes(w: &mut Work<'_>, rng: &mut Rng) -> Result<bool> {
    let data = w.get(VERTEXES)?;
    let Some(n) = count(&data, VERTEX_LEN) else { return Ok(false) };
    let mut edges = w.get(EDGES)?;
    if !(3..=65_535).contains(&n) || count(&edges, EDGE_LEN).is_none() {
        return Ok(false);
    }
    let new_to_old = shuffled_identity(n, 0, rng);
    let map = inverse(&new_to_old);
    if !remap_u16_field(&mut edges, EDGE_LEN, 0, &map, false) || !remap_u16_field(&mut edges, EDGE_LEN, 2, &map, false)
    {
        return Ok(false);
    }
    w.set(VERTEXES, permute_records(&data, VERTEX_LEN, &new_to_old));
    w.set(EDGES, edges);
    Ok(true)
}

fn edges(w: &mut Work<'_>, rng: &mut Rng) -> Result<bool> {
    let data = w.get(EDGES)?;
    let Some(n) = count(&data, EDGE_LEN) else { return Ok(false) };
    let mut surf = w.get(SURFEDGES)?;
    if n < 4 || count(&surf, 4).is_none() {
        return Ok(false);
    }
    let new_to_old = shuffled_identity(n, 1, rng);
    let map = inverse(&new_to_old);
    for r in 0..surf.len() / 4 {
        let v = i32_at(&surf, r * 4);
        let Some(&m) = map.get(v.unsigned_abs() as usize) else { return Ok(false) };
        put32(&mut surf, r * 4, if v < 0 { -(m as i32) } else { m as i32 });
    }
    w.set(EDGES, permute_records(&data, EDGE_LEN, &new_to_old));
    w.set(SURFEDGES, surf);
    Ok(true)
}

fn brushes(w: &mut Work<'_>, rng: &mut Rng) -> Result<bool> {
    let data = w.get(BRUSHES)?;
    let sides = w.get(BRUSHSIDES)?;
    let mut leaf_brushes = w.get(LEAFBRUSHES)?;
    let (Some(n), Some(ns), Some(_)) = (count(&data, BRUSH_LEN), count(&sides, SIDE_LEN), count(&leaf_brushes, 2))
    else {
        return Ok(false);
    };
    if !(3..=65_535).contains(&n) {
        return Ok(false);
    }
    let mut blocks: Vec<(usize, usize, usize)> = Vec::with_capacity(n);
    for b in 0..n {
        let first = i32_at(&data, b * BRUSH_LEN);
        let num = i32_at(&data, b * BRUSH_LEN + 4);
        let (Ok(first), Ok(num)) = (usize::try_from(first), usize::try_from(num)) else { return Ok(false) };
        if first + num > ns {
            return Ok(false);
        }
        blocks.push((first, num, b));
    }
    let mut by_start = blocks.clone();
    by_start.sort();
    let mut end = 0usize;
    for &(first, num, _) in &by_start {
        if first < end {
            return Ok(false);
        }
        end = first + num;
    }
    let new_to_old = shuffled_identity(n, 0, rng);
    let map = inverse(&new_to_old);
    let mut new_sides: Vec<u8> = Vec::with_capacity(sides.len());
    let mut new_brushes = data.clone();
    let mut covered = vec![false; ns];
    for (new, &old) in new_to_old.iter().enumerate() {
        let (first, num, _) = blocks[old];
        put32(&mut new_brushes, new * BRUSH_LEN, (new_sides.len() / SIDE_LEN) as i32);
        put32(&mut new_brushes, new * BRUSH_LEN + 4, num as i32);
        put32(&mut new_brushes, new * BRUSH_LEN + 8, i32_at(&data, old * BRUSH_LEN + 8));
        new_sides.extend_from_slice(&sides[first * SIDE_LEN..(first + num) * SIDE_LEN]);
        covered[first..first + num].iter_mut().for_each(|c| *c = true);
    }
    for (s, c) in covered.iter().enumerate() {
        if !c {
            new_sides.extend_from_slice(&sides[s * SIDE_LEN..(s + 1) * SIDE_LEN]);
        }
    }
    if !remap_u16_field(&mut leaf_brushes, 2, 0, &map, false) {
        return Ok(false);
    }
    w.set(BRUSHES, new_brushes);
    w.set(BRUSHSIDES, new_sides);
    w.set(LEAFBRUSHES, leaf_brushes);
    Ok(true)
}

fn faces(w: &mut Work<'_>, rng: &mut Rng) -> Result<bool> {
    let data = w.get(lump::FACES)?;
    let Some(n) = count(&data, FACE_LEN) else { return Ok(false) };
    let nodes = w.get(NODES)?;
    let models = w.get(MODELS)?;
    let hdr = w.get(FACES_HDR)?;
    let ids = w.get(FACEIDS)?;
    let macro_info = w.get(FACE_MACRO)?;
    let normal_indices = w.get(VERTNORMALINDICES)?;
    let mut leaf_faces = w.get(LEAFFACES)?;
    let mut overlays = w.get(lump::OVERLAYS)?;
    let mut water_overlays = w.get(WATEROVERLAYS)?;
    let mut disps = w.get(DISPINFO)?;
    if !(4..=65_535).contains(&n)
        || count(&nodes, NODE_LEN).is_none()
        || count(&models, MODEL_LEN).is_none()
        || count(&overlays, OVERLAY_LEN).is_none()
        || count(&water_overlays, WATER_OVERLAY_LEN).is_none()
        || count(&disps, DISP_LEN).is_none()
        || leaf_faces.len() % 2 != 0
        || !(hdr.is_empty() || hdr.len() == data.len())
        || !(ids.is_empty() || ids.len() == n * 2)
        || !(macro_info.is_empty() || macro_info.len() == n * 2)
    {
        return Ok(false);
    }
    let mut edge_counts: Vec<usize> = Vec::with_capacity(n);
    for f in 0..n {
        let Ok(c) = usize::try_from(i16::from_le_bytes([data[f * FACE_LEN + 8], data[f * FACE_LEN + 9]])) else {
            return Ok(false);
        };
        edge_counts.push(c);
    }
    if !(normal_indices.is_empty() || normal_indices.len() == edge_counts.iter().sum::<usize>() * 2) {
        return Ok(false);
    }
    let mut spans: Vec<(usize, usize)> = Vec::new();
    for i in 0..nodes.len() / NODE_LEN {
        let first = u16_at(&nodes, i * NODE_LEN + 24) as usize;
        let num = u16_at(&nodes, i * NODE_LEN + 26) as usize;
        if num > 1 {
            spans.push((first, num));
        }
    }
    spans.sort();
    let mut end = 0usize;
    for &(first, num) in &spans {
        if first < end || first + num > n {
            return Ok(false);
        }
        end = first + num;
    }
    for m in 0..models.len() / MODEL_LEN {
        let mf = i32_at(&models, m * MODEL_LEN + 40);
        let mn = i32_at(&models, m * MODEL_LEN + 44);
        let (Ok(mf), Ok(mn)) = (usize::try_from(mf), usize::try_from(mn)) else { return Ok(false) };
        for &(first, num) in &spans {
            let inside = first >= mf && first + num <= mf + mn;
            let outside = first + num <= mf || first >= mf + mn;
            if !inside && !outside {
                return Ok(false);
            }
        }
    }
    let mut new_to_old: Vec<usize> = (0..n).collect();
    for &(first, num) in &spans {
        rng.shuffle(&mut new_to_old[first..first + num]);
    }
    let map = inverse(&new_to_old);
    if !remap_u16_field(&mut leaf_faces, 2, 0, &map, false) || !remap_u16_field(&mut disps, DISP_LEN, 36, &map, false) {
        return Ok(false);
    }
    for (list, len, most) in [(&mut overlays, OVERLAY_LEN, 64), (&mut water_overlays, WATER_OVERLAY_LEN, 256)] {
        for o in 0..list.len() / len {
            let base = o * len;
            let cnt = (u16_at(list, base + 6) & 0x3fff) as usize;
            if cnt > most {
                return Ok(false);
            }
            for k in 0..cnt {
                let p = base + 8 + k * 4;
                let v = i32_at(list, p);
                let Some(&m) = usize::try_from(v).ok().and_then(|v| map.get(v)) else { return Ok(false) };
                put32(list, p, m as i32);
            }
        }
    }
    w.set(lump::FACES, permute_records(&data, FACE_LEN, &new_to_old));
    if !hdr.is_empty() {
        w.set(FACES_HDR, permute_records(&hdr, FACE_LEN, &new_to_old));
    }
    if !ids.is_empty() {
        w.set(FACEIDS, permute_records(&ids, 2, &new_to_old));
    }
    if !macro_info.is_empty() {
        w.set(FACE_MACRO, permute_records(&macro_info, 2, &new_to_old));
    }
    if !normal_indices.is_empty() {
        let mut starts = Vec::with_capacity(n);
        let mut at = 0usize;
        for &c in &edge_counts {
            starts.push(at);
            at += c * 2;
        }
        let mut moved = Vec::with_capacity(normal_indices.len());
        for &old in &new_to_old {
            moved.extend_from_slice(&normal_indices[starts[old]..starts[old] + edge_counts[old] * 2]);
        }
        w.set(VERTNORMALINDICES, moved);
    }
    w.set(LEAFFACES, leaf_faces);
    w.set(lump::OVERLAYS, overlays);
    if !water_overlays.is_empty() {
        w.set(WATEROVERLAYS, water_overlays);
    }
    w.set(DISPINFO, disps);
    Ok(true)
}

pub type ReorderedLumps = Vec<(usize, Vec<u8>)>;

pub fn reorder(bsp: &Bsp, opts: ReorderOptions, rng: &mut Rng) -> Result<(ReorderedLumps, ReorderReport)> {
    let mut w = Work { bsp, cur: HashMap::new() };
    let mut report = ReorderReport::default();
    let mut run = |name: &'static str, f: fn(&mut Work<'_>, &mut Rng) -> Result<bool>, w: &mut Work<'_>| match f(w, rng)
    {
        Ok(true) => report.applied.push(name),
        _ => report.skipped.push(name),
    };
    if opts.tables {
        run("planes", planes, &mut w);
        run("texinfo", texinfo, &mut w);
        run("texdata", texdata, &mut w);
        run("vertexes", vertexes, &mut w);
        run("edges", edges, &mut w);
    }
    if opts.world {
        run("brushes", brushes, &mut w);
        run("faces", faces, &mut w);
    }
    let mut out: Vec<(usize, Vec<u8>)> = Vec::new();
    for (i, data) in w.cur {
        if bsp.lump(i)?.as_ref() != data.as_slice() {
            out.push((i, data));
        }
    }
    out.sort_by_key(|(i, _)| *i);
    Ok((out, report))
}
