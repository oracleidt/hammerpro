use crate::gamelump::id;
use crate::header::lump;
use crate::layout::{LayoutOptions, relayout};
use crate::reorder::{ReorderOptions, reorder};
use crate::rng::Rng;
use crate::*;

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn i32_at(b: &[u8], at: usize) -> i32 {
    i32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn f32s(rng: &mut Rng, n: usize) -> Vec<u8> {
    (0..n).flat_map(|_| ((rng.below(20000) as f32) / 10.0 - 1000.0).to_le_bytes()).collect()
}

fn push16(v: &mut Vec<u8>, x: i64) {
    v.extend_from_slice(&(x as i16).to_le_bytes());
}

fn push32(v: &mut Vec<u8>, x: i64) {
    v.extend_from_slice(&(x as i32).to_le_bytes());
}

fn world_lumps(seed: u64) -> Vec<(usize, Vec<u8>)> {
    let mut rng = Rng::from_u64(seed);
    let (np, nv, ne, ns, nt, ni, nf, nn, nl, nb) = (40usize, 50, 60, 120, 6, 10, 30, 8, 9, 12);
    let mut planes = Vec::new();
    for _ in 0..np {
        planes.extend(f32s(&mut rng, 4));
        push32(&mut planes, rng.below(6) as i64);
    }
    let vertexes = f32s(&mut rng, nv * 3);
    let mut edges = vec![0u8; 4];
    for _ in 1..ne {
        push16(&mut edges, rng.below(nv) as i64);
        push16(&mut edges, rng.below(nv) as i64);
    }
    let mut surf = Vec::new();
    for _ in 0..ns {
        let e = 1 + rng.below(ne - 1) as i64;
        push32(&mut surf, if rng.below(2) == 0 { e } else { -e });
    }
    let mut texdata = Vec::new();
    let mut names = Vec::new();
    let mut table = Vec::new();
    for t in 0..nt {
        texdata.extend(f32s(&mut rng, 3));
        push32(&mut texdata, t as i64);
        push32(&mut texdata, 64 + t as i64);
        push32(&mut texdata, 128);
        push32(&mut texdata, 64 + t as i64);
        push32(&mut texdata, 128);
        push32(&mut table, names.len() as i64);
        names.extend_from_slice(format!("test/mat{t}\0").as_bytes());
    }
    let mut texinfo = Vec::new();
    for _ in 0..ni {
        texinfo.extend(f32s(&mut rng, 16));
        push32(&mut texinfo, rng.below(1000) as i64);
        push32(&mut texinfo, rng.below(nt) as i64);
    }
    let mut faces = Vec::new();
    let mut hdr = Vec::new();
    for f in 0..nf {
        for out in [&mut faces, &mut hdr] {
            let mut r = Rng::from_u64(seed + f as u64 * 7 + out.len() as u64 % 3);
            push16(out, rng.below(np) as i64);
            out.extend_from_slice(&[rng.below(2) as u8, 1]);
            push32(out, (f * 3 % (ns - 4)) as i64);
            push16(out, 3);
            push16(out, rng.below(ni) as i64);
            push16(
                out,
                if f == 5 {
                    0
                } else if f == 9 {
                    1
                } else {
                    -1
                },
            );
            push16(out, -1);
            out.extend_from_slice(&[0, 255, 255, 255]);
            push32(out, r.below(100000) as i64);
            out.extend(f32s(&mut r, 1));
            out.extend(vec![0u8; 56 - out.len() % 56]);
        }
    }
    let faceids: Vec<u8> = (0..nf * 2).map(|i| i as u8).collect();
    let mut nodes = Vec::new();
    for n in 0..nn {
        push32(&mut nodes, rng.below(np) as i64);
        push32(&mut nodes, -1 - (n as i64));
        push32(&mut nodes, -2);
        nodes.extend(vec![0u8; 12]);
        push16(&mut nodes, (n * 3) as i64);
        push16(&mut nodes, 3);
        push16(&mut nodes, 0);
        push16(&mut nodes, 0);
    }
    let mut leaffaces = Vec::new();
    let mut leafbrushes = Vec::new();
    let mut leafs = Vec::new();
    for l in 0..nl {
        push32(&mut leafs, l as i64);
        push16(&mut leafs, l as i64);
        push16(&mut leafs, 0);
        leafs.extend(vec![0u8; 12]);
        push16(&mut leafs, (leaffaces.len() / 2) as i64);
        let k = 2 + l % 3;
        push16(&mut leafs, k as i64);
        for _ in 0..k {
            push16(&mut leaffaces, rng.below(nf) as i64);
        }
        push16(&mut leafs, (leafbrushes.len() / 2) as i64);
        let k = 1 + l % 4;
        push16(&mut leafs, k as i64);
        for _ in 0..k {
            push16(&mut leafbrushes, rng.below(nb) as i64);
        }
        push16(&mut leafs, -1);
        push16(&mut leafs, 0);
    }
    let mut brushes = Vec::new();
    let mut sides = Vec::new();
    for b in 0..nb {
        push32(&mut brushes, (sides.len() / 8) as i64);
        push32(&mut brushes, 6);
        push32(&mut brushes, 1 + b as i64);
        for _ in 0..6 {
            push16(&mut sides, rng.below(np) as i64);
            push16(&mut sides, rng.below(ni) as i64 - 1);
            push16(&mut sides, -1);
            push16(&mut sides, rng.below(2) as i64);
        }
    }
    let mut models = Vec::new();
    for (first, num) in [(0usize, 24usize), (24, 6)] {
        models.extend(f32s(&mut rng, 9));
        push32(&mut models, 0);
        push32(&mut models, first as i64);
        push32(&mut models, num as i64);
    }
    let mut disps = Vec::new();
    for map_face in [5usize, 9] {
        let mut d = vec![0u8; 176];
        d[32..34].copy_from_slice(&(map_face as u16).to_le_bytes());
        disps.extend(d);
    }
    let mut overlays = Vec::new();
    for o in 0..2usize {
        let mut ov = vec![0u8; 352];
        ov[0..4].copy_from_slice(&(o as i32).to_le_bytes());
        ov[4..6].copy_from_slice(&(rng.below(ni) as i16).to_le_bytes());
        ov[6..8].copy_from_slice(&3u16.to_le_bytes());
        for k in 0..3 {
            ov[8 + k * 4..12 + k * 4].copy_from_slice(&(rng.below(nf) as i32).to_le_bytes());
        }
        overlays.extend(ov);
    }
    let mut portals = Vec::new();
    for _ in 0..2 {
        portals.extend(vec![0u8; 8]);
        push32(&mut portals, rng.below(np) as i64);
    }
    let mut occ = Vec::new();
    push32(&mut occ, 1);
    push32(&mut occ, 0);
    push32(&mut occ, 0);
    push32(&mut occ, 2);
    occ.extend(vec![0u8; 24]);
    push32(&mut occ, 3);
    push32(&mut occ, 2);
    for _ in 0..2 {
        push16(&mut occ, 0);
        push16(&mut occ, 4);
        push32(&mut occ, rng.below(np) as i64);
    }
    push32(&mut occ, 4);
    for i in 0..4 {
        push32(&mut occ, i);
    }
    let mut water = Vec::new();
    water.extend(vec![0u8; 8]);
    push32(&mut water, rng.below(ni) as i64);
    vec![
        (lump::ENTITIES, b"{\n\"classname\" \"worldspawn\"\n}\n\0".to_vec()),
        (1, planes),
        (lump::TEXDATA, texdata),
        (3, vertexes),
        (5, nodes),
        (6, texinfo),
        (lump::FACES, faces),
        (lump::LEAFS, leafs),
        (11, faceids),
        (12, edges),
        (13, surf),
        (lump::MODELS, models),
        (16, leaffaces),
        (17, leafbrushes),
        (18, brushes),
        (19, sides),
        (21, portals),
        (9, occ),
        (26, disps),
        (36, water),
        (lump::OVERLAYS, overlays),
        (lump::FACES_HDR, hdr),
        (lump::TEXDATA_STRING_DATA, names),
        (lump::TEXDATA_STRING_TABLE, table),
        (lump::GAME_LUMP, {
            let mut g = GameLumps::default();
            g.set(id::STATIC_PROPS, 10, vec![1; 40]).unwrap();
            g.set(id::DETAIL_PROPS, 4, vec![2; 24]).unwrap();
            g.set(id::DETAIL_PROP_LIGHTING, 1, vec![3; 12]).unwrap();
            g.to_bytes(0).unwrap()
        }),
        (lump::PAKFILE, {
            let mut w = PakWriter::new();
            w.add("materials/test/mat0.vmt", b"\"LightmappedGeneric\"\n{\n}\n".to_vec()).unwrap();
            w.finish(PakOptions::default()).unwrap()
        }),
    ]
}

fn world_bsp(seed: u64) -> Bsp {
    let bytes = synthetic_bsp(20, &world_lumps(seed));
    let mut header = Header::parse(&bytes).unwrap();
    header.lumps[lump::LEAFS].version = 1;
    header.lumps[9].version = 2;
    let mut out = bytes;
    out[..HEADER_LEN].copy_from_slice(&header.to_bytes());
    Bsp::from_vec(out).unwrap()
}

struct View {
    bsp: Bsp,
}

impl View {
    fn l(&self, i: usize) -> Vec<u8> {
        self.bsp.lump(i).unwrap().into_owned()
    }

    fn plane(&self, i: usize) -> Vec<u8> {
        self.l(1)[i * 20..i * 20 + 20].to_vec()
    }

    fn texinfo(&self, i: i64) -> Vec<u8> {
        if i < 0 {
            return vec![0xff];
        }
        let i = i as usize;
        let ti = self.l(6)[i * 72..i * 72 + 72].to_vec();
        let td = i32_at(&ti, 68) as usize;
        let mut out = ti[..68].to_vec();
        let d = self.l(2)[td * 32..td * 32 + 32].to_vec();
        out.extend_from_slice(&d[..12]);
        out.extend_from_slice(&d[16..]);
        let name_id = i32_at(&d, 12) as usize;
        let table = self.l(44);
        let ofs = i32_at(&table, name_id * 4) as usize;
        let data = self.l(43);
        let end = data[ofs..].iter().position(|&b| b == 0).unwrap();
        out.extend_from_slice(&data[ofs..ofs + end]);
        out
    }

    fn vertex(&self, i: usize) -> Vec<u8> {
        self.l(3)[i * 12..i * 12 + 12].to_vec()
    }

    fn face(&self, lump_id: usize, f: usize) -> Vec<u8> {
        let rec = self.l(lump_id);
        let r = &rec[f * 56..f * 56 + 56];
        let mut out = self.plane(u16_at(r, 0) as usize);
        out.extend_from_slice(&r[2..4]);
        out.extend_from_slice(&r[8..10]);
        out.extend_from_slice(&self.texinfo(i64::from(u16_at(r, 10) as i16)));
        out.extend_from_slice(&r[12..56]);
        let surf = self.l(13);
        let edges = self.l(12);
        let first = i32_at(r, 4) as usize;
        for k in 0..u16_at(r, 8) as usize {
            let s = i32_at(&surf, (first + k) * 4);
            let e = s.unsigned_abs() as usize;
            let v = if s < 0 { 1 } else { 0 };
            out.extend(self.vertex(u16_at(&edges, e * 4 + v * 2) as usize));
        }
        out
    }

    fn face_all(&self, f: usize) -> Vec<u8> {
        let mut out = self.face(lump::FACES, f);
        if !self.l(lump::FACES_HDR).is_empty() {
            out.extend(self.face(lump::FACES_HDR, f));
        }
        let ids = self.l(11);
        if !ids.is_empty() {
            out.extend_from_slice(&ids[f * 2..f * 2 + 2]);
        }
        out
    }

    fn brush(&self, b: usize) -> Vec<u8> {
        let br = self.l(18);
        let sides = self.l(19);
        let mut out = br[b * 12 + 8..b * 12 + 12].to_vec();
        let first = i32_at(&br, b * 12) as usize;
        for s in first..first + i32_at(&br, b * 12 + 4) as usize {
            let r = &sides[s * 8..s * 8 + 8];
            out.extend(self.plane(u16_at(r, 0) as usize));
            out.extend(self.texinfo(i64::from(u16_at(r, 2) as i16)));
            out.extend_from_slice(&r[4..8]);
        }
        out
    }

    fn faces_of(&self, first: usize, num: usize) -> Vec<Vec<u8>> {
        let mut v: Vec<Vec<u8>> = (first..first + num).map(|f| self.face_all(f)).collect();
        v.sort();
        v
    }

    fn digest(&self) -> Vec<Vec<Vec<u8>>> {
        let mut out = self.core_digest();
        let occ = self.l(9);
        let polys = i32_at(&occ, 4 + 40) as usize;
        out.push((0..polys).map(|p| self.plane(i32_at(&occ, 4 + 40 + 4 + p * 8 + 4) as usize)).collect());
        let water = self.l(36);
        out.push(vec![self.texinfo(i64::from(i32_at(&water, 8)))]);
        out
    }

    fn core_digest(&self) -> Vec<Vec<Vec<u8>>> {
        let mut out: Vec<Vec<Vec<u8>>> = Vec::new();
        let nf = self.l(7).len() / 56;
        out.push(self.faces_of(0, nf));
        let nb = self.l(18).len() / 12;
        let mut brushes: Vec<Vec<u8>> = (0..nb).map(|b| self.brush(b)).collect();
        brushes.sort();
        out.push(brushes);
        let leafs = self.l(10);
        let lf = self.l(16);
        let lb = self.l(17);
        for i in 0..leafs.len() / 32 {
            let r = &leafs[i * 32..i * 32 + 32];
            let mut faces: Vec<Vec<u8>> = (0..u16_at(r, 22) as usize)
                .map(|k| self.face_all(u16_at(&lf, (u16_at(r, 20) as usize + k) * 2) as usize))
                .collect();
            faces.sort();
            let mut br: Vec<Vec<u8>> = (0..u16_at(r, 26) as usize)
                .map(|k| self.brush(u16_at(&lb, (u16_at(r, 24) as usize + k) * 2) as usize))
                .collect();
            br.sort();
            out.push(faces);
            out.push(br);
            out.push(vec![r[..20].to_vec(), r[28..].to_vec()]);
        }
        let nodes = self.l(5);
        for i in 0..nodes.len() / 32 {
            let r = &nodes[i * 32..i * 32 + 32];
            out.push(vec![self.plane(i32_at(r, 0) as usize), r[4..24].to_vec()]);
            out.push(self.faces_of(u16_at(r, 24) as usize, u16_at(r, 26) as usize));
        }
        let models = self.l(14);
        for m in 0..models.len() / 48 {
            out.push(self.faces_of(i32_at(&models, m * 48 + 40) as usize, i32_at(&models, m * 48 + 44) as usize));
        }
        let disps = self.l(26);
        for d in 0..disps.len() / 176 {
            out.push(vec![self.face_all(u16_at(&disps, d * 176 + 32) as usize)]);
        }
        let ov = self.l(45);
        for o in 0..ov.len() / 352 {
            let r = &ov[o * 352..o * 352 + 352];
            let mut v = vec![self.texinfo(i64::from(u16_at(r, 4) as i16))];
            for k in 0..(u16_at(r, 6) & 0x3fff) as usize {
                v.push(self.face_all(i32_at(r, 8 + k * 4) as usize));
            }
            out.push(v);
        }
        let portals = self.l(21);
        out.push((0..portals.len() / 12).map(|p| self.plane(i32_at(&portals, p * 12 + 8) as usize)).collect());
        out
    }
}

fn all_reorder() -> ReorderOptions {
    ReorderOptions { tables: true, world: true }
}

fn apply_reorder(bsp: &Bsp, seed: u64) -> (Bsp, crate::reorder::ReorderReport) {
    let mut rng = Rng::from_u64(seed);
    let (changed, rep) = reorder(bsp, all_reorder(), &mut rng).unwrap();
    let mut edit = Bsp::from_vec(bsp.bytes().to_vec()).unwrap().edit();
    for (i, data) in changed {
        edit.replace_lump(i, data).unwrap();
    }
    (Bsp::from_vec(edit.to_vec().unwrap()).unwrap(), rep)
}

#[test]
fn the_synthetic_world_is_well_formed() {
    let v = View { bsp: world_bsp(1) };
    assert_eq!(v.digest().len(), 2 + 9 * 3 + 8 * 2 + 2 + 2 + 2 + 3);
}

#[test]
fn reordering_keeps_every_resolved_structure() {
    for seed in 1..40u64 {
        let before = View { bsp: world_bsp(seed) };
        let (after_bsp, rep) = apply_reorder(&before.bsp, seed * 31);
        assert_eq!(
            rep.applied,
            vec!["planes", "texinfo", "texdata", "vertexes", "edges", "brushes", "faces", "leaf_lists"],
            "skipped: {:?}",
            rep.skipped
        );
        let after = View { bsp: after_bsp };
        assert_eq!(before.digest(), after.digest(), "seed {seed}");
        let moved = [1usize, 6, 2, 3, 12, 18, 7, 16].iter().filter(|&&i| before.l(i) != after.l(i)).count();
        assert!(moved >= 6, "seed {seed}: only {moved} lumps changed");
    }
}

#[test]
fn reordering_is_deterministic_and_seed_sensitive() {
    let b = world_bsp(3);
    let (x, _) = apply_reorder(&b, 5);
    let (y, _) = apply_reorder(&b, 5);
    let (z, _) = apply_reorder(&b, 6);
    assert_eq!(x.bytes(), y.bytes());
    assert_ne!(x.bytes(), z.bytes());
}

#[test]
fn plane_pairs_stay_adjacent() {
    let b = world_bsp(4);
    let (after, _) = apply_reorder(&b, 9);
    let planes = after.lump(1).unwrap().into_owned();
    let orig = b.lump(1).unwrap().into_owned();
    for p in (0..planes.len() / 20).step_by(2) {
        let first = &planes[p * 20..p * 20 + 20];
        let at = (0..orig.len() / 20).find(|&o| &orig[o * 20..o * 20 + 20] == first).unwrap();
        assert_eq!(at % 2, 0);
        assert_eq!(&planes[(p + 1) * 20..(p + 1) * 20 + 20], &orig[(at + 1) * 20..(at + 1) * 20 + 20]);
    }
}

#[test]
fn broken_references_skip_the_transform_without_damage() {
    let mut lumps = world_lumps(2);
    let sides = lumps.iter_mut().find(|(i, _)| *i == 19).unwrap();
    sides.1[0..2].copy_from_slice(&60000u16.to_le_bytes());
    let bytes = synthetic_bsp(20, &lumps);
    let bsp = Bsp::from_vec(bytes).unwrap();
    let mut rng = Rng::from_u64(1);
    let (changed, rep) = reorder(&bsp, all_reorder(), &mut rng).unwrap();
    assert!(rep.skipped.contains(&"planes"), "{rep:?}");
    assert!(changed.iter().all(|(i, _)| *i != 1), "planes must stay as they were");
}

fn lump_contents(bsp: &Bsp) -> Vec<(usize, Vec<u8>)> {
    (0..LUMP_COUNT)
        .filter(|&i| bsp.header().lumps[i].len > 0)
        .map(|i| {
            if i == lump::GAME_LUMP {
                let mut g: Vec<(u32, Vec<u8>)> =
                    bsp.game_lumps().unwrap().lumps.iter().map(|l| (l.id, l.stored.clone())).collect();
                g.sort();
                (i, g.into_iter().flat_map(|(id, d)| id.to_le_bytes().into_iter().chain(d)).collect())
            } else {
                (i, bsp.lump_raw(i).unwrap().to_vec())
            }
        })
        .collect()
}

#[test]
fn a_shuffled_layout_reads_back_identical() {
    let base = world_bsp(7);
    let want = lump_contents(&base);
    for seed in 0..25u64 {
        let mut rng = Rng::from_u64(seed);
        let (bytes, rep) = relayout(base.bytes(), &LayoutOptions::strongest(), &mut rng).unwrap();
        let got = Bsp::from_vec(bytes.clone()).unwrap();
        assert_eq!(lump_contents(&got), want, "seed {seed}");
        assert!(rep.decoy_blocks >= 4 && bytes.len() > base.len());
        for i in 0..LUMP_COUNT {
            let l = got.header().lumps[i];
            assert_eq!(l.offset % 4, 0, "lump {i} misaligned");
            assert!(l.len == 0 || l.offset as usize + l.len as usize <= bytes.len());
        }
        assert_eq!(got.entities().unwrap()[0].classname(), "worldspawn");
        assert_eq!(got.pakfile().unwrap().len(), 1);
        assert_eq!(got.game_lumps().unwrap().lumps.len(), 3);
    }
}

#[test]
fn the_physical_order_really_changes() {
    let base = world_bsp(7);
    let order = |bytes: &[u8]| {
        let b = Bsp::from_vec(bytes.to_vec()).unwrap();
        let mut ids: Vec<usize> = (0..LUMP_COUNT).filter(|&i| b.header().lumps[i].len > 0).collect();
        ids.sort_by_key(|&i| b.header().lumps[i].offset);
        ids
    };
    let opts = LayoutOptions { shuffle_lumps: true, ..Default::default() };
    let plain = order(base.bytes());
    let mut seen = std::collections::HashSet::new();
    for seed in 0..10u64 {
        let (bytes, _) = relayout(base.bytes(), &opts, &mut Rng::from_u64(seed)).unwrap();
        seen.insert(order(&bytes));
    }
    assert!(seen.len() >= 8, "orders are random per seed");
    assert!(!seen.contains(&plain) || seen.len() > 1);
}

#[test]
fn decoys_are_not_referenced_and_ignored_by_the_reader() {
    let base = world_bsp(8);
    let opts = LayoutOptions { decoys: true, ..Default::default() };
    let (bytes, rep) = relayout(base.bytes(), &opts, &mut Rng::from_u64(3)).unwrap();
    assert!(rep.decoy_blocks >= 4);
    let got = Bsp::from_vec(bytes.clone()).unwrap();
    assert_eq!(lump_contents(&got), lump_contents(&base));
    let referenced: usize = (0..LUMP_COUNT).map(|i| got.header().lumps[i].len as usize).sum();
    assert!(referenced < bytes.len());
    assert!(bytes.len() >= base.len() + rep.decoy_bytes);
    let pak_end = got.header().lumps[lump::PAKFILE];
    let end = pak_end.offset as usize + pak_end.len as usize;
    assert!(bytes.len() - end < 4, "pak stays last without random_pak");
}

#[test]
fn the_pakfile_lands_at_random_positions() {
    let base = world_bsp(8);
    let opts = LayoutOptions { random_pak: true, ..Default::default() };
    let mut positions = std::collections::HashSet::new();
    for seed in 0..30u64 {
        let (bytes, rep) = relayout(base.bytes(), &opts, &mut Rng::from_u64(seed)).unwrap();
        positions.insert(rep.pak_index.unwrap());
        assert_eq!(Bsp::from_vec(bytes).unwrap().pakfile().unwrap().len(), 1);
    }
    assert!(positions.len() >= 5, "{positions:?}");
}

#[test]
fn game_lump_directory_order_is_shuffled_and_readable() {
    let base = world_bsp(9);
    let opts = LayoutOptions { shuffle_game_lumps: true, ..Default::default() };
    let mut orders = std::collections::HashSet::new();
    for seed in 0..20u64 {
        let (bytes, _) = relayout(base.bytes(), &opts, &mut Rng::from_u64(seed)).unwrap();
        let got = Bsp::from_vec(bytes).unwrap();
        let g = got.game_lumps().unwrap();
        assert_eq!(g.get(id::STATIC_PROPS).unwrap().stored, vec![1; 40]);
        assert_eq!(g.get(id::DETAIL_PROP_LIGHTING).unwrap().stored, vec![3; 12]);
        orders.insert(g.lumps.iter().map(|l| l.id).collect::<Vec<_>>());
    }
    assert!(orders.len() >= 4, "{orders:?}");
}

#[test]
fn header_tricks_only_touch_what_the_engine_ignores() {
    let base = world_bsp(10);
    let opts = LayoutOptions { scramble_header: true, ..Default::default() };
    let (bytes, _) = relayout(base.bytes(), &opts, &mut Rng::from_u64(11)).unwrap();
    let got = Bsp::from_vec(bytes.clone()).unwrap();
    assert_eq!(got.version(), 20);
    assert_ne!(got.map_revision(), base.map_revision());
    for i in 0..LUMP_COUNT {
        let (a, b) = (base.header().lumps[i], got.header().lumps[i]);
        assert_eq!(a.len, b.len);
        if a.len > 0 {
            assert_eq!(a.four_cc, b.four_cc, "lump {i} compression marker");
            assert!(matches!(i, 0..=6 | 12..=14 | 16..=21 | 40 | 43 | 44) || a.version == b.version, "lump {i}");
        }
    }
    assert!(!got.any_compressed());
    assert_eq!(lump_contents(&got), lump_contents(&base));
    let versions_that_matter = [lump::LEAFS, 9, lump::GAME_LUMP, lump::OVERLAYS, lump::FACES_HDR];
    for i in versions_that_matter {
        assert_eq!(base.header().lumps[i].version, got.header().lumps[i].version, "lump {i}");
    }
}

#[test]
fn layout_is_deterministic_per_seed_and_a_pure_function_of_it() {
    let base = world_bsp(12);
    let run = |seed: u64| relayout(base.bytes(), &LayoutOptions::strongest(), &mut Rng::from_u64(seed)).unwrap().0;
    assert_eq!(run(5), run(5));
    assert_ne!(run(5), run(6));
}

#[test]
fn relayout_is_idempotent_in_content() {
    let base = world_bsp(13);
    let (once, _) = relayout(base.bytes(), &LayoutOptions::strongest(), &mut Rng::from_u64(1)).unwrap();
    let (twice, _) = relayout(&once, &LayoutOptions::strongest(), &mut Rng::from_u64(2)).unwrap();
    let a = Bsp::from_vec(once).unwrap();
    let b = Bsp::from_vec(twice).unwrap();
    assert_eq!(lump_contents(&a), lump_contents(&b));
    assert_eq!(lump_contents(&a), lump_contents(&base));
    let (c, _) = relayout(base.bytes(), &LayoutOptions::default(), &mut Rng::from_u64(1)).unwrap();
    assert_eq!(c, base.bytes());
}

#[test]
fn a_shuffled_bsp_survives_an_edit_round_trip() {
    let base = world_bsp(14);
    let (bytes, _) = relayout(base.bytes(), &LayoutOptions::strongest(), &mut Rng::from_u64(1)).unwrap();
    let again = Bsp::from_vec(bytes).unwrap().edit().to_vec().unwrap();
    assert_eq!(lump_contents(&Bsp::from_vec(again).unwrap()), lump_contents(&base));
}

#[test]
fn protect_with_every_option_keeps_the_map_and_reports_it() {
    let base = world_bsp(15);
    let mut opts = ProtectOptions::strongest();
    opts.layout_seed = "fixed seed".into();
    opts.randomize_materials = false;
    let (a, ra) = protect(&base, &opts).unwrap();
    let (b, rb) = protect(&base, &opts).unwrap();
    assert_eq!(a, b, "the seed fixes the layout");
    assert_eq!(ra.layout_seed, "fixed seed");
    assert_eq!(ra, rb);
    assert!(ra.decoy_blocks >= 4 && ra.gap_bytes > 0);
    assert_eq!(ra.reordered.len(), 8, "{:?}", ra.reordered);
    let before = View { bsp: world_bsp(15) };
    let after = View { bsp: Bsp::from_vec(a.clone()).unwrap() };
    let (bd, ad) = (before.digest(), after.digest());
    for (i, (b, a)) in bd.iter().zip(ad.iter()).enumerate() {
        assert_eq!(b, a, "structure {i}");
    }
    opts.layout_seed = String::new();
    let (c, rc) = protect(&base, &opts).unwrap();
    let (d, rd) = protect(&base, &opts).unwrap();
    assert_ne!(c, d, "an empty seed is random per compile");
    assert_ne!(rc.layout_seed, rd.layout_seed);
    let server = server_copy(&c, &base, &opts).unwrap();
    let client = Bsp::from_vec(c).unwrap();
    let server = Bsp::from_vec(server).unwrap();
    for i in (0..LUMP_COUNT).filter(|&i| i != lump::ENTITIES && i != lump::GAME_LUMP) {
        assert_eq!(client.lump_raw(i).unwrap(), server.lump_raw(i).unwrap(), "lump {i}");
    }
    assert_eq!(client.map_revision(), server.map_revision());
}

#[test]
fn real_map_reorder_and_layout_keep_the_resolved_world() {
    let path = std::path::Path::new("E:/hp-scratch/agent-protect/work/hpt_src.bsp");
    let Ok(bsp) = Bsp::read(path) else { return };
    let before = View { bsp: Bsp::from_vec(bsp.bytes().to_vec()).unwrap() }.core_digest();
    for seed in 1..4u64 {
        let mut opts = ProtectOptions::strongest();
        opts.layout_seed = format!("real{seed}");
        opts.randomize_materials = false;
        let (bytes, rep) = protect(&bsp, &opts).unwrap();
        assert_eq!(rep.reordered.len(), 8, "{:?}", rep.reordered);
        let after = View { bsp: Bsp::from_vec(bytes).unwrap() }.core_digest();
        assert_eq!(before.len(), after.len());
        for (i, (b, a)) in before.iter().zip(after.iter()).enumerate() {
            assert_eq!(b, a, "structure {i} differs");
        }
    }
}
