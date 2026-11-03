use crate::procmem::MemSource;
use crate::scan::{ScanOptions, par_chunks};

pub const OCCLUDER_RECORD: usize = 40;
pub const POLY_RECORD: usize = 12;
pub const OCCLUSION_VERSION: u16 = 2;
pub const LUMP_INDEX: usize = 9;
const GAP_STEPS: usize = 24;
const MAX_POLYS_PER_OCCLUDER: i32 = 1024;
const MAX_POLY_VERTS: i32 = 64;
const MAX_AREA: i32 = 65_535;
const PLANE_RECORD: usize = 20;
const ON_PLANE: f32 = 0.5;
const MAX_COORD: f32 = 1.0e6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Occluder {
    pub flags: i32,
    pub first_poly: i32,
    pub poly_count: i32,
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    pub area: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OccluderPoly {
    pub first_vertex: i32,
    pub vertex_count: i32,
    pub plane: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MemOcclusion {
    pub occluders: Vec<Occluder>,
    pub polys: Vec<OccluderPoly>,
    pub indices: Vec<i32>,
}

pub struct WorldRefs<'a> {
    pub plane_bytes: &'a [u8],
    pub vertexes: &'a [[f32; 3]],
}

fn i32_at(d: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn f32_at(d: &[u8], at: usize) -> f32 {
    f32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

pub fn parse_occluder(rec: &[u8]) -> Option<Occluder> {
    if rec.len() < OCCLUDER_RECORD {
        return None;
    }
    let flags = i32_at(rec, 0);
    let first_poly = i32_at(rec, 4);
    let poly_count = i32_at(rec, 8);
    let mut mins = [0f32; 3];
    let mut maxs = [0f32; 3];
    for k in 0..3 {
        mins[k] = f32_at(rec, 12 + k * 4);
        maxs[k] = f32_at(rec, 24 + k * 4);
        if !mins[k].is_finite() || !maxs[k].is_finite() || mins[k].abs() > MAX_COORD || maxs[k].abs() > MAX_COORD {
            return None;
        }
        if mins[k] > maxs[k] {
            return None;
        }
    }
    let area = i32_at(rec, 36);
    if !(0..=0xffff).contains(&flags) || first_poly < 0 || !(1..=MAX_POLYS_PER_OCCLUDER).contains(&poly_count) {
        return None;
    }
    if !(0..=MAX_AREA).contains(&area) {
        return None;
    }
    Some(Occluder { flags, first_poly, poly_count, mins, maxs, area })
}

fn plane_of(refs: &WorldRefs<'_>, n: i32) -> Option<[f32; 4]> {
    let at = usize::try_from(n).ok()? * PLANE_RECORD;
    let p = refs.plane_bytes.get(at..at + 16)?;
    Some([f32_at(p, 0), f32_at(p, 4), f32_at(p, 8), f32_at(p, 12)])
}

fn parse_polys(data: &[u8], at: usize, total: usize, refs: &WorldRefs<'_>) -> Option<Vec<OccluderPoly>> {
    let end = at.checked_add(total * POLY_RECORD)?;
    let raw = data.get(at..end)?;
    let mut polys = Vec::with_capacity(total);
    let mut next_vertex = 0i32;
    for c in raw.chunks_exact(POLY_RECORD) {
        let p = OccluderPoly { first_vertex: i32_at(c, 0), vertex_count: i32_at(c, 4), plane: i32_at(c, 8) };
        if p.first_vertex != next_vertex || !(3..=MAX_POLY_VERTS).contains(&p.vertex_count) {
            return None;
        }
        plane_of(refs, p.plane)?;
        next_vertex += p.vertex_count;
        polys.push(p);
    }
    Some(polys)
}

fn parse_indices(data: &[u8], at: usize, polys: &[OccluderPoly], refs: &WorldRefs<'_>) -> Option<Vec<i32>> {
    let total: usize = polys.iter().map(|p| p.vertex_count as usize).sum();
    let raw = data.get(at..at.checked_add(total * 4)?)?;
    let indices: Vec<i32> = raw.chunks_exact(4).map(|c| i32_at(c, 0)).collect();
    if indices.iter().any(|&i| i < 0 || i as usize >= refs.vertexes.len()) {
        return None;
    }
    for p in polys {
        let plane = plane_of(refs, p.plane)?;
        let ring = &indices[p.first_vertex as usize..(p.first_vertex + p.vertex_count) as usize];
        let mut distinct: Vec<i32> = ring.to_vec();
        distinct.sort_unstable();
        distinct.dedup();
        if distinct.len() != ring.len() {
            return None;
        }
        for k in 0..p.vertex_count as usize {
            let v = refs.vertexes[indices[p.first_vertex as usize + k] as usize];
            let d = plane[0] * v[0] + plane[1] * v[1] + plane[2] * v[2] - plane[3];
            if d.abs() > ON_PLANE {
                return None;
            }
        }
    }
    Some(indices)
}

pub fn parse_occlusion_at(data: &[u8], refs: &WorldRefs<'_>) -> Option<MemOcclusion> {
    let first = parse_occluder(data)?;
    if first.first_poly != 0 {
        return None;
    }
    let mut chain = vec![first];
    let mut want = first.first_poly + first.poly_count;
    loop {
        let at = chain.len() * OCCLUDER_RECORD;
        let Some(rec) = data.get(at..at + OCCLUDER_RECORD) else { break };
        match parse_occluder(rec) {
            Some(o) if o.first_poly == want => {
                want += o.poly_count;
                chain.push(o);
            }
            _ => break,
        }
    }
    for n in (1..=chain.len()).rev() {
        let list = &chain[..n];
        let total: i32 = list.iter().map(|o| o.poly_count).sum();
        let after = n * OCCLUDER_RECORD;
        for gap in 0..GAP_STEPS {
            let polys_at = after + gap * 4;
            let Some(polys) = parse_polys(data, polys_at, total as usize, refs) else { continue };
            let polys_end = polys_at + polys.len() * POLY_RECORD;
            for gap2 in 0..GAP_STEPS {
                if let Some(indices) = parse_indices(data, polys_end + gap2 * 4, &polys, refs) {
                    return Some(MemOcclusion { occluders: list.to_vec(), polys, indices });
                }
            }
        }
    }
    None
}

pub fn find_occlusion<S: MemSource + ?Sized>(
    src: &S,
    opts: &ScanOptions,
    refs: &WorldRefs<'_>,
) -> Option<MemOcclusion> {
    if refs.plane_bytes.is_empty() || refs.vertexes.is_empty() {
        return None;
    }
    let opts = ScanOptions { overlap: 1 << 18, writable_only: true, ..*opts };
    let found: Vec<(u64, MemOcclusion)> = par_chunks(src, &opts, |base, data, own| {
        let mut out = Vec::new();
        let limit = own.min(data.len());
        let mut j = 0usize;
        while j + OCCLUDER_RECORD <= data.len() && j < limit {
            if i32_at(data, j + 4) == 0
                && (1..=MAX_POLYS_PER_OCCLUDER).contains(&i32_at(data, j + 8))
                && let Some(o) = parse_occlusion_at(&data[j..], refs)
            {
                out.push((base + j as u64, o));
            }
            j += 4;
        }
        out
    })
    .into_iter()
    .flatten()
    .collect();
    found.into_iter().max_by_key(|(addr, o)| (o.occluders.len(), std::cmp::Reverse(*addr))).map(|(_, o)| o)
}

pub fn read_arrays<S: MemSource + ?Sized>(
    src: &S,
    occluders: (u64, i64),
    polys: (u64, i64),
    indices: (u64, i64),
) -> Option<MemOcclusion> {
    let (n, p, v) = (usize::try_from(occluders.1).ok()?, usize::try_from(polys.1).ok()?, usize::try_from(indices.1).ok()?);
    if n == 0 || n > 65_536 || p > 1 << 20 || v > 1 << 22 || occluders.0 == 0 {
        return None;
    }
    let raw_o = src.read_vec(occluders.0, n * OCCLUDER_RECORD);
    let raw_p = if p == 0 { Vec::new() } else { src.read_vec(polys.0, p * POLY_RECORD) };
    let raw_i = if v == 0 { Vec::new() } else { src.read_vec(indices.0, v * 4) };
    if raw_o.len() != n * OCCLUDER_RECORD || raw_p.len() != p * POLY_RECORD || raw_i.len() != v * 4 {
        return None;
    }
    let occ: Option<Vec<Occluder>> = raw_o.chunks_exact(OCCLUDER_RECORD).map(parse_occluder).collect();
    let polys: Vec<OccluderPoly> = raw_p
        .chunks_exact(POLY_RECORD)
        .map(|c| OccluderPoly { first_vertex: i32_at(c, 0), vertex_count: i32_at(c, 4), plane: i32_at(c, 8) })
        .collect();
    let indices: Vec<i32> = raw_i.chunks_exact(4).map(|c| i32_at(c, 0)).collect();
    Some(MemOcclusion { occluders: occ?, polys, indices })
}

pub fn occlusion_lump(o: &MemOcclusion) -> Vec<u8> {
    let mut out =
        Vec::with_capacity(12 + o.occluders.len() * OCCLUDER_RECORD + o.polys.len() * POLY_RECORD + o.indices.len() * 4);
    out.extend_from_slice(&(o.occluders.len() as i32).to_le_bytes());
    for c in &o.occluders {
        out.extend_from_slice(&c.flags.to_le_bytes());
        out.extend_from_slice(&c.first_poly.to_le_bytes());
        out.extend_from_slice(&c.poly_count.to_le_bytes());
        for v in c.mins.iter().chain(&c.maxs) {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&c.area.to_le_bytes());
    }
    out.extend_from_slice(&(o.polys.len() as i32).to_le_bytes());
    for p in &o.polys {
        out.extend_from_slice(&p.first_vertex.to_le_bytes());
        out.extend_from_slice(&p.vertex_count.to_le_bytes());
        out.extend_from_slice(&p.plane.to_le_bytes());
    }
    out.extend_from_slice(&(o.indices.len() as i32).to_le_bytes());
    for i in &o.indices {
        out.extend_from_slice(&i.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::procmem::VecMem;

    pub(crate) fn world() -> (Vec<u8>, Vec<[f32; 3]>) {
        let mut planes = Vec::new();
        for (n, d) in [([0.0f32, 0.0, 1.0], 8.0f32), ([1.0, 0.0, 0.0], 64.0), ([0.0, 1.0, 0.0], 32.0)] {
            for v in n {
                planes.extend_from_slice(&v.to_le_bytes());
            }
            planes.extend_from_slice(&d.to_le_bytes());
            planes.extend_from_slice(&0i32.to_le_bytes());
        }
        let verts = vec![
            [0.0, 0.0, 8.0],
            [64.0, 0.0, 8.0],
            [64.0, 64.0, 8.0],
            [0.0, 64.0, 8.0],
            [64.0, 0.0, 0.0],
            [64.0, 64.0, 0.0],
            [64.0, 64.0, 64.0],
            [64.0, 0.0, 64.0],
        ];
        (planes, verts)
    }

    pub(crate) fn sample() -> MemOcclusion {
        MemOcclusion {
            occluders: vec![
                Occluder { flags: 0, first_poly: 0, poly_count: 1, mins: [0.0, 0.0, 8.0], maxs: [64.0, 64.0, 8.0], area: 3 },
                Occluder {
                    flags: 1,
                    first_poly: 1,
                    poly_count: 1,
                    mins: [64.0, 0.0, 0.0],
                    maxs: [64.0, 64.0, 64.0],
                    area: 4,
                },
            ],
            polys: vec![
                OccluderPoly { first_vertex: 0, vertex_count: 4, plane: 0 },
                OccluderPoly { first_vertex: 4, vertex_count: 4, plane: 1 },
            ],
            indices: vec![0, 1, 2, 3, 4, 5, 6, 7],
        }
    }

    fn lay_out(o: &MemOcclusion, gap1: usize, gap2: usize) -> Vec<u8> {
        let lump = occlusion_lump(o);
        let n = o.occluders.len();
        let mut out = Vec::new();
        out.extend_from_slice(&lump[4..4 + n * OCCLUDER_RECORD]);
        let p0 = 4 + n * OCCLUDER_RECORD + 4;
        out.extend(vec![0u8; gap1]);
        out.extend_from_slice(&lump[p0..p0 + o.polys.len() * POLY_RECORD]);
        let i0 = p0 + o.polys.len() * POLY_RECORD + 4;
        out.extend(vec![0u8; gap2]);
        out.extend_from_slice(&lump[i0..]);
        out
    }

    #[test]
    fn the_three_arrays_are_found_with_gaps_between_them() {
        let (planes, verts) = world();
        let refs = WorldRefs { plane_bytes: &planes, vertexes: &verts };
        let o = sample();
        for (g1, g2) in [(0, 0), (0, 8), (16, 4), (64, 32)] {
            let mut data = vec![0xeeu8; 36];
            data.extend(lay_out(&o, g1, g2));
            data.extend(vec![0x55u8; 40]);
            let mut mem = VecMem::default();
            mem.add(0x6000_0000, data);
            let found = find_occlusion(&mem, &ScanOptions::default(), &refs);
            assert_eq!(found.as_ref(), Some(&o), "gaps {g1} {g2}");
        }
    }

    #[test]
    fn the_engine_arrays_are_read_through_their_pointers() {
        let o = sample();
        let lump = occlusion_lump(&o);
        let mut mem = VecMem::default();
        mem.add(0x1000, lump[4..4 + 80].to_vec());
        mem.add(0x2000, lump[4 + 80 + 4..4 + 80 + 4 + 24].to_vec());
        mem.add(0x3000, lump[4 + 80 + 4 + 24 + 4..].to_vec());
        let got = read_arrays(&mem, (0x1000, 2), (0x2000, 2), (0x3000, 8)).expect("arrays");
        assert_eq!(got, o);
        assert!(read_arrays(&mem, (0x1000, 0), (0x2000, 2), (0x3000, 8)).is_none());
        assert!(read_arrays(&mem, (0x9000, 2), (0x2000, 2), (0x3000, 8)).is_none());
    }

    #[test]
    fn the_lump_has_the_game_layout() {
        let o = sample();
        let lump = occlusion_lump(&o);
        assert_eq!(lump.len(), 4 + 2 * 40 + 4 + 2 * 12 + 4 + 8 * 4);
        assert_eq!(i32_at(&lump, 0), 2);
        assert_eq!(i32_at(&lump, 4 + 80), 2);
        assert_eq!(i32_at(&lump, 4 + 80 + 4 + 24), 8);
    }

    #[test]
    fn a_polygon_that_is_off_its_plane_is_refused() {
        let (planes, mut verts) = world();
        verts[2][2] = 20.0;
        let refs = WorldRefs { plane_bytes: &planes, vertexes: &verts };
        let mut data = lay_out(&sample(), 0, 0);
        data.extend(vec![0u8; 64]);
        let got = parse_occlusion_at(&data, &refs);
        assert!(got.is_none(), "{got:?}");
    }

    #[test]
    fn random_bytes_are_not_an_occluder() {
        let (planes, verts) = world();
        let refs = WorldRefs { plane_bytes: &planes, vertexes: &verts };
        let junk: Vec<u8> = (0..4096u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 11) as u8).collect();
        let mut mem = VecMem::default();
        mem.add(0x6000_0000, junk);
        assert!(find_occlusion(&mem, &ScanOptions::default(), &refs).is_none());
    }
}
