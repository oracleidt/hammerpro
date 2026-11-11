pub mod vbsp;

use glam::DVec3;

use crate::emit::Buf;
use crate::winding::Winding;

#[derive(Clone, Debug)]
pub struct DispFace {
    pub winding: Winding,
    pub plane: usize,
    pub texinfo: i32,
    pub info: hp_vmf::DispInfo,
    pub contents: u32,
    pub ids: (u32, u32),
    pub surfaceprop: String,
    pub surfaceprop2: Option<String>,
    pub four_way: bool,
}

pub const ALPHA_PROP_DELTA: f64 = 382.5;

pub fn collision_triangles(d: &DispFace) -> Vec<([DVec3; 3], f64)> {
    let Some(core) = vbsp::core(d) else { return Vec::new() };
    let tris = vbsp::tesselate(&core, &|k| allowed(d, k));
    tris.iter()
        .map(|t| {
            let p = t.map(|k| {
                DVec3::new(f64::from(core.verts[k][0]), f64::from(core.verts[k][1]), f64::from(core.verts[k][2]))
            });
            let alpha: f64 = t.iter().map(|&k| f64::from(core.alphas[k]) / 255.0).sum();
            (p, alpha)
        })
        .collect()
}

fn allowed(d: &DispFace, k: usize) -> bool {
    d.info.allowed_verts.get(k / 32).is_none_or(|w| (*w as u32 >> (k % 32)) & 1 != 0)
}

const ALL_VISIBLE_CONTENTS: u32 = 0xFF;

pub fn physdisp_lump(disps: &[DispFace], mask: u32) -> Vec<u8> {
    use rayon::prelude::*;
    let blobs: Vec<Option<Vec<u8>>> = disps
        .par_iter()
        .map(|d| {
            let mut contents = d.contents;
            if contents
                & (ALL_VISIBLE_CONTENTS
                    | crate::materials::contents::PLAYERCLIP
                    | crate::materials::contents::MONSTERCLIP)
                == 0
            {
                contents |= crate::materials::contents::SOLID;
            }
            if contents & mask == 0 || d.info.flags & hp_geom::DISP_FLAG_NO_PHYSICS != 0 {
                return None;
            }
            let core = vbsp::core(d)?;
            let ok = |k: usize| allowed(d, k);
            let (_, verts) = vbsp::snap(&core, &ok);
            let mut idx: Vec<u16> = vbsp::tesselate(&core, &ok).iter().flatten().map(|&k| k as u16).collect();
            idx.reverse();
            let max = usize::from(*idx.iter().max()?);
            crate::phys::vmesh::virtual_mesh(&verts[..=max], &idx)
        })
        .collect();
    let mut b = Buf::default();
    b.u16(disps.len() as u16);
    for blob in &blobs {
        b.u16(blob.as_ref().map_or(0xFFFF, |x| x.len() as u16));
    }
    for blob in blobs.into_iter().flatten() {
        b.0.extend_from_slice(&blob);
    }
    b.0
}

pub fn lightmap(d: &DispFace, lightmap: &[[f32; 4]; 2]) -> Option<([i32; 2], Vec<u8>)> {
    let core = vbsp::core(d)?;
    let (size, _, _) = vbsp::luxel_size(&core, lightmap)?;
    Some((size, vbsp::lightmap_samples(&core, lightmap)))
}

pub fn disp_data(d: &DispFace, _offset: DVec3) -> Option<(Vec<u8>, Vec<u16>)> {
    let core = vbsp::core(d)?;
    let mut verts = Buf::default();
    for (v, dist, alpha) in vbsp::dispverts(&core, &|k| allowed(d, k)) {
        for c in v {
            verts.f32(c);
        }
        verts.f32(dist);
        verts.f32(alpha);
    }
    Some((verts.0, compiled_tags(&d.info)))
}

fn compiled_tags(info: &hp_vmf::DispInfo) -> Vec<u16> {
    let count = (1usize << (2 * info.power.min(4))) * 2;
    (0..count)
        .map(|k| {
            let t = info.triangle_tags.get(k).copied().unwrap_or(0) as u16;
            let walkable = if t & 2 != 0 { t & 4 != 0 } else { t & 1 != 0 };
            let buildable = if t & 16 != 0 { t & 32 != 0 } else { t & 8 != 0 };
            let removed = t & 64 != 0;
            (u16::from(walkable) * 2) | (u16::from(buildable) * 4) | (u16::from(removed) * 32)
        })
        .collect()
}

pub fn setup_allowed_verts(disps: &mut [DispFace]) {
    struct Grid {
        n: usize,
        base: Vec<DVec3>,
        allowed: Vec<bool>,
    }
    let mut grids: Vec<Option<Grid>> = disps
        .iter()
        .map(|d| {
            let ccw: Vec<DVec3> = d.winding.points.iter().rev().copied().collect();
            let mesh = hp_geom::build_disp(&ccw, &d.info)?;
            let n = mesh.row_len;
            (n >= 2 && mesh.base_positions.len() >= n * n).then(|| Grid {
                n,
                base: mesh.base_positions.clone(),
                allowed: vec![true; n * n],
            })
        })
        .collect();
    let edges = |g: &Grid| -> [(DVec3, DVec3, [usize; 2]); 4] {
        let n = g.n;
        let k = |r: usize, c: usize| r * n + c;
        [
            (g.base[k(0, 0)], g.base[k(0, n - 1)], [k(0, 0), 1]),
            (g.base[k(0, n - 1)], g.base[k(n - 1, n - 1)], [k(0, n - 1), n]),
            (g.base[k(n - 1, n - 1)], g.base[k(n - 1, 0)], [k(n - 1, n - 1), usize::MAX]),
            (g.base[k(n - 1, 0)], g.base[k(0, 0)], [k(n - 1, 0), usize::MAX - 1]),
        ]
    };
    let edge_vert = |n: usize, e: usize, i: usize| -> usize {
        match e {
            0 => i,
            1 => i * n + (n - 1),
            2 => (n - 1) * n + (n - 1 - i),
            _ => (n - 1 - i) * n,
        }
    };
    const EPS: f64 = 0.1;
    let boxes: Vec<Option<(DVec3, DVec3)>> = grids
        .iter()
        .map(|g| {
            g.as_ref().map(|g| {
                let corners = edges(g).map(|e| e.0);
                let lo = corners.iter().fold(DVec3::splat(f64::MAX), |m, c| m.min(*c)) - DVec3::splat(2.0 * EPS);
                let hi = corners.iter().fold(DVec3::splat(f64::MIN), |m, c| m.max(*c)) + DVec3::splat(2.0 * EPS);
                (lo, hi)
            })
        })
        .collect();
    let cores: Vec<_> = disps.iter().map(vbsp::core).collect();
    let neighbors = vbsp::neighbors(&cores);
    let edge_neighbors = |a: usize, b: usize| {
        neighbors.get(a).is_some_and(|n| n.edges.iter().flatten().any(|s| usize::from(s.index) == b))
    };
    let mut order: Vec<usize> = (0..grids.len()).filter(|&i| boxes[i].is_some()).collect();
    order.sort_by(|&x, &y| boxes[x].unwrap().0.x.total_cmp(&boxes[y].unwrap().0.x));
    let mut near_of: Vec<Vec<usize>> = vec![Vec::new(); grids.len()];
    for (k, &x) in order.iter().enumerate() {
        let (xlo, xhi) = boxes[x].unwrap();
        for &y in &order[k + 1..] {
            let (ylo, yhi) = boxes[y].unwrap();
            if ylo.x > xhi.x {
                break;
            }
            if xlo.cmple(yhi).all() && ylo.cmple(xhi).all() {
                near_of[x].push(y);
                near_of[y].push(x);
            }
        }
    }
    for v in &mut near_of {
        v.sort_unstable();
    }
    loop {
        let mut changed = false;
        for a in 0..grids.len() {
            let Some(ga) = grids[a].as_ref() else { continue };
            let na = ga.n;
            let mut clear = Vec::new();
            for (ea, &(a0, a1, _)) in edges(ga).iter().enumerate() {
                let dir = a1 - a0;
                let len = dir.length();
                if len < 1e-6 {
                    continue;
                }
                let u = dir / len;
                for i in 1..na - 1 {
                    let va = edge_vert(na, ea, i);
                    if !ga.allowed[va] {
                        continue;
                    }
                    let p = ga.base[va];
                    for &b in &near_of[a] {
                        if !edge_neighbors(a, b) {
                            continue;
                        }
                        let Some(gb) = grids[b].as_ref() else { continue };
                        for (eb, &(b0, b1, _)) in edges(gb).iter().enumerate() {
                            let bd = b1 - b0;
                            let bl = bd.length();
                            if bl < 1e-6 || (bd / bl).cross(u).length() > 1e-3 {
                                continue;
                            }
                            let near = |x: DVec3, y: DVec3| (x - y).length() <= EPS;
                            let same = |p0: DVec3, p1: DVec3, q0: DVec3, q1: DVec3| {
                                (near(p0, q0) && near(p1, q1)) || (near(p0, q1) && near(p1, q0))
                            };
                            let (am, bm) = ((a0 + a1) * 0.5, (b0 + b1) * 0.5);
                            let neighbours = same(a0, a1, b0, b1)
                                || same(a0, am, b0, b1)
                                || same(am, a1, b0, b1)
                                || same(b0, bm, a0, a1)
                                || same(bm, b1, a0, a1);
                            if !neighbours {
                                continue;
                            }
                            let t = (p - b0).dot(bd) / (bl * bl);
                            if !(-1e-6..=1.0 + 1e-6).contains(&t) || (b0 + bd * t - p).length() > EPS {
                                continue;
                            }
                            let f = t * (gb.n - 1) as f64;
                            let step = f.round();
                            let ok = (f - step).abs() * bl / (gb.n - 1) as f64 <= EPS
                                && gb.allowed[edge_vert(gb.n, eb, step as usize)];
                            if !ok {
                                clear.push(va);
                            }
                        }
                    }
                }
            }
            if !clear.is_empty()
                && let Some(ga) = grids[a].as_mut()
            {
                for v in clear {
                    if ga.allowed[v] {
                        ga.allowed[v] = false;
                        changed = true;
                    }
                }
            }
        }
        if !changed {
            break;
        }
    }
    for (d, g) in disps.iter_mut().zip(&grids) {
        let mut words = vec![-1i32; 10];
        if let Some(g) = g {
            for (k, ok) in g.allowed.iter().enumerate() {
                if !ok {
                    words[k / 32] &= !(1i32 << (k % 32));
                }
            }
        }
        d.info.allowed_verts = words;
    }
}

pub const NO_SHADOW_BIT: i32 = 1 << 29;
pub const HAS_MULTIBLEND_BIT: i32 = 1 << 30;
pub const MULTIBLEND_ENTRY_SIZE: usize = 80;

pub fn multiblend_data(d: &DispFace) -> Option<Vec<u8>> {
    let n = d.info.row_len();
    let count = n.checked_mul(n).filter(|_| d.info.power <= hp_vmf::MAX_DISP_POWER)?;
    let blend = match hp_vmf::multiblend::MultiBlend::of(&d.info) {
        Some(mb) => mb,
        None if d.four_way => hp_vmf::multiblend::MultiBlend::from_alphas(&d.info)?,
        None => return None,
    };
    let finite = |v: f64| if v.is_finite() { v as f32 } else { 0.0 };
    let mut b = Buf::default();
    for k in 0..count {
        for v in blend.weights.get(k).copied().unwrap_or([1.0, 0.0, 0.0, 0.0]) {
            b.f32(finite(v));
        }
        for v in blend.alphas.get(k).copied().unwrap_or([0.0; 4]) {
            b.f32(finite(v));
        }
        for colors in &blend.colors {
            for v in colors.get(k).copied().unwrap_or([1.0; 3]) {
                b.f32(finite(v));
            }
        }
    }
    debug_assert_eq!(b.0.len(), count * MULTIBLEND_ENTRY_SIZE);
    Some(b.0)
}

pub fn strip_no_shadow(lump: &[u8]) -> Option<Vec<u8>> {
    let mut out = lump.to_vec();
    let mut any = false;
    for rec in out.chunks_exact_mut(176) {
        let b = rec.get_mut(24..28)?;
        let v = i32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        if v < 0 && v & NO_SHADOW_BIT != 0 {
            b.copy_from_slice(&(v & !NO_SHADOW_BIT).to_le_bytes());
            any = true;
        }
    }
    any.then_some(out)
}

#[allow(clippy::too_many_arguments)]
pub fn dispinfo_record(
    d: &DispFace,
    offset: DVec3,
    vert_start: i32,
    tri_start: i32,
    map_face: u16,
    samples_start: i32,
    neighbors: &vbsp::Neighbors,
    multiblend: bool,
) -> Vec<u8> {
    let mut b = Buf::default();
    b.vec3(d.info.start_position + offset);
    b.i32(vert_start);
    b.i32(tri_start);
    b.i32(d.info.power as i32);
    let no_shadow = if d.info.casts_shadows() { 0 } else { NO_SHADOW_BIT };
    let multiblend = if multiblend { HAS_MULTIBLEND_BIT } else { 0 };
    b.i32((d.info.flags & !HAS_MULTIBLEND_BIT) | i32::MIN | no_shadow | multiblend);
    let smooth = d
        .info
        .extra_props
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("smooth"))
        .and_then(|(_, v)| v.trim().parse::<f32>().ok());
    b.f32(smooth.unwrap_or(0.0));
    b.i32(d.contents as i32);
    b.u16(map_face);
    b.u16(0);
    b.i32(0);
    b.i32(samples_start);
    for edge in &neighbors.edges {
        for sub in edge {
            b.u16(sub.index);
            b.u8(sub.orientation);
            b.u8(sub.span);
            b.u8(sub.neighbor_span);
            b.u8(0);
        }
    }
    for corner in &neighbors.corners {
        for k in 0..4 {
            b.u16(corner.get(k).copied().unwrap_or(0));
        }
        b.u8(corner.len() as u8);
        b.u8(0);
    }
    for k in 0..10 {
        let v = d.info.allowed_verts.get(k).copied().unwrap_or(-1);
        b.i32(v);
    }
    debug_assert_eq!(b.0.len(), 176);
    b.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn floor(tags: Vec<i32>) -> DispFace {
        let cw = vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(0.0, 128.0, 0.0),
            DVec3::new(128.0, 128.0, 0.0),
            DVec3::new(128.0, 0.0, 0.0),
        ];
        let ccw: Vec<DVec3> = cw.iter().rev().copied().collect();
        let mut info = hp_geom::create_disp(&ccw, 2).expect("disp");
        info.triangle_tags = tags;
        DispFace {
            winding: Winding::new(cw),
            plane: 0,
            texinfo: 0,
            info,
            contents: 1,
            ids: (1, 1),
            surfaceprop: String::new(),
            surfaceprop2: None,
            four_way: false,
        }
    }

    fn tags_of(d: &DispFace) -> Vec<u16> {
        disp_data(d, DVec3::ZERO).expect("surface").1
    }

    #[test]
    fn triangle_tags_are_collapsed_like_vbsp() {
        assert_eq!(tags_of(&floor(vec![9; 32])), vec![6; 32]);
        assert_eq!(tags_of(&floor(vec![9 | 2; 32])), vec![4; 32]);
        assert_eq!(tags_of(&floor(vec![9 | 16; 32])), vec![2; 32]);
        assert_eq!(tags_of(&floor(vec![6; 32])), vec![2; 32]);
        assert_eq!(tags_of(&floor(vec![9 | 64; 32])), vec![6 | 32; 32]);
        let walk_on = 2 | 4;
        let build_on = 16 | 32;
        let mut steep = floor(vec![walk_on | build_on; 32]);
        steep.winding = Winding::new(vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(0.0, 0.0, 128.0),
            DVec3::new(0.0, 128.0, 128.0),
            DVec3::new(0.0, 128.0, 0.0),
        ]);
        let ccw: Vec<DVec3> = steep.winding.points.iter().rev().copied().collect();
        let mut info = hp_geom::create_disp(&ccw, 2).expect("wall disp");
        info.triangle_tags = vec![0; 32];
        steep.info = info.clone();
        assert_eq!(tags_of(&steep), vec![0; 32], "a wall is neither walkable nor buildable");
        info.triangle_tags = vec![walk_on | build_on; 32];
        steep.info = info;
        assert_eq!(tags_of(&steep), vec![6; 32], "forced on");
        assert_eq!(tags_of(&floor(Vec::new())), vec![0; 32]);
        assert_eq!(tags_of(&floor(vec![0; 32])), vec![0; 32]);
    }

    fn fnv(d: &[u8]) -> u64 {
        d.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &x| (h ^ u64::from(x)).wrapping_mul(0x0100_0000_01b3))
    }

    fn canonical(raw: &[u8]) -> Vec<u8> {
        let mut out = raw.to_vec();
        for x in out.chunks_exact_mut(176) {
            x[38] = 0;
            x[39] = 0;
            for s in 0..8 {
                let o = 48 + 6 * s;
                if x[o] == 0xff && x[o + 1] == 0xff {
                    x[o + 2..o + 6].fill(0);
                }
                x[o + 5] = 0;
            }
            for k in 0..4 {
                let o = 96 + 10 * k;
                let n = usize::from(x[o + 8]).min(4);
                x[o + 2 * n..o + 8].fill(0);
                x[o + 9] = 0;
            }
        }
        out
    }

    struct Mats;
    impl crate::materials::MaterialSource for Mats {
        fn facts(&self, name: &str) -> crate::materials::MaterialFacts {
            crate::materials::facts_by_name(name)
        }
    }

    #[test]
    fn multiblend_entries_follow_the_rows() {
        let mut d = floor(Vec::new());
        assert_eq!(multiblend_data(&d), None, "plain displacement");
        d.info.alphas[3] = 255.0;
        d.four_way = true;
        let synth = multiblend_data(&d).expect("4-way material: from the alphas");
        assert_eq!(synth.len(), 25 * MULTIBLEND_ENTRY_SIZE);
        let at = |data: &[u8], k: usize, f: usize| {
            let o = k * MULTIBLEND_ENTRY_SIZE + f * 4;
            f32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]])
        };
        assert_eq!((0..4).map(|f| at(&synth, 3, f)).collect::<Vec<_>>(), [0.0, 1.0, 0.0, 0.0]);
        assert_eq!((0..4).map(|f| at(&synth, 0, f)).collect::<Vec<_>>(), [1.0, 0.0, 0.0, 0.0]);
        let mut mb = hp_vmf::multiblend::MultiBlend::new(25);
        mb.weights[7] = [0.25, 0.5, 2.0, 0.0];
        mb.alphas[7] = [0.1, 0.2, 0.3, 0.4];
        mb.colors[2][7] = [0.5, 0.6, 0.7];
        assert!(mb.write_to(&mut d.info));
        d.four_way = false;
        let data = multiblend_data(&d).expect("rows");
        let entry: Vec<f32> = (0..20).map(|f| at(&data, 7, f)).collect();
        assert_eq!(&entry[..8], &[0.25, 0.5, 2.0, 0.0, 0.1, 0.2, 0.3, 0.4], "copied like vbsp, not normalized");
        assert_eq!(&entry[8..11], &[1.0; 3]);
        assert_eq!(&entry[14..17], &[0.5, 0.6, 0.7]);
        let n = vbsp::Neighbors::default();
        let tess = |rec: &[u8]| i32::from_le_bytes([rec[24], rec[25], rec[26], rec[27]]);
        assert_eq!(tess(&dispinfo_record(&d, DVec3::ZERO, 0, 0, 0, 0, &n, true)), i32::MIN | HAS_MULTIBLEND_BIT);
        assert_eq!(tess(&dispinfo_record(&d, DVec3::ZERO, 0, 0, 0, 0, &n, false)), i32::MIN);
    }

    #[test]
    fn multiblend_lump_matches_vbsp() {
        let map = hp_vmf::parse(include_str!("disp/testdata/multiblend.vmf")).expect("vmf");
        let out = crate::compile(&map, &Mats, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |_| {})
            .expect("compile");
        assert!(crate::validate::validate(&out.bsp).is_empty());
        let bsp = hp_bsp::Bsp::from_vec(out.bsp.clone()).expect("bsp");
        let blend = bsp.lump(hp_bsp::lump::DISP_MULTIBLEND).expect("lump").into_owned();
        let mut cut = hp_bsp::Bsp::from_vec(out.bsp).expect("bsp").edit();
        cut.replace_lump(hp_bsp::lump::DISP_MULTIBLEND, blend[..80].to_vec()).expect("replace");
        let problems = crate::validate::validate(&cut.to_vec().expect("bytes"));
        assert!(problems.iter().any(|p| p.contains("multiblend")), "{problems:?}");
        assert_eq!(blend, include_bytes!("disp/testdata/multiblend_vbsp.lump"), "Garry's Mod vbsp output");
        let info = bsp.lump(crate::emit::lump::DISPINFO).expect("dispinfo").into_owned();
        assert_eq!(i32::from_le_bytes([info[24], info[25], info[26], info[27]]), i32::MIN | HAS_MULTIBLEND_BIT);
        let back = hp_decompile::decompile(&bsp, &hp_decompile::Options::default()).expect("decompile");
        let disp = back.map.world.solids.iter().flat_map(|s| &s.sides).find_map(|s| s.disp.as_deref()).expect("disp");
        let source = map.world.solids[0].sides[0].disp.as_deref().expect("source disp");
        assert_eq!(hp_vmf::multiblend::MultiBlend::of(disp), hp_vmf::multiblend::MultiBlend::of(source), "decompiled");
        let options = crate::CompileOptions { format: hp_bsp::Branch::Left4Dead2.format(), ..Default::default() };
        let out = crate::compile(&map, &Mats, &crate::props::NoModels, &options, &mut |_| {}).expect("compile");
        let bsp = hp_bsp::Bsp::from_vec(out.bsp).expect("bsp");
        assert!(bsp.lump(hp_bsp::lump::DISP_MULTIBLEND).expect("lump").is_empty(), "no multiblend in that branch");
        let info = bsp.lump(crate::emit::lump::DISPINFO).expect("dispinfo").into_owned();
        assert_eq!(i32::from_le_bytes([info[24], info[25], info[26], info[27]]), i32::MIN);
    }

    #[test]
    fn displacements_match_vbsp() {
        let map = hp_vmf::parse(include_str!("disp/testdata/disp_golden.vmf")).expect("vmf");
        let out = crate::compile(&map, &Mats, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |_| {})
            .expect("compile");
        let bsp = hp_bsp::Bsp::from_vec(out.bsp).expect("bsp");
        let lump = |i: usize| bsp.lump(i).expect("lump").into_owned();
        use crate::emit::lump as l;
        assert_eq!((lump(l::PHYSDISP).len(), fnv(&lump(l::PHYSDISP))), (1457, 0x6b63_8cc6_b39d_72fe), "PHYSDISP");
        assert_eq!((lump(l::DISP_VERTS).len(), fnv(&lump(l::DISP_VERTS))), (5860, 0x3e88_0cbf_a568_6c15), "DISP_VERTS");
        assert_eq!(
            (lump(l::DISP_LIGHTMAP_SAMPLE_POSITIONS).len(), fnv(&lump(l::DISP_LIGHTMAP_SAMPLE_POSITIONS))),
            (38368, 0x4663_3b20_672b_73c8),
            "DISP_LIGHTMAP_SAMPLE_POSITIONS"
        );
        assert_eq!((lump(l::DISP_TRIS).len(), fnv(&lump(l::DISP_TRIS))), (896, 0x4687_7bc4_e4cc_d033), "DISP_TRIS");
        let info = canonical(&lump(l::DISPINFO));
        assert_eq!((info.len(), fnv(&info)), (880, 0xd0a4_cdb8_5329_c8e0), "DISPINFO");
    }

    #[test]
    fn a_displaced_brush_keeps_only_its_displacement_like_vbsp() {
        let map = hp_vmf::parse(include_str!("disp/testdata/disp_golden.vmf")).expect("vmf");
        let mut flat = map.clone();
        flat.world.solids.retain(|s| s.sides.iter().all(|side| side.disp.is_none()));
        assert!(flat.world.solids.len() < map.world.solids.len(), "the fixture has displaced brushes");
        let counts = |m: &hp_vmf::Map| {
            let out = crate::compile(m, &Mats, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |_| {})
                .expect("compile");
            let bsp = hp_bsp::Bsp::from_vec(out.bsp).expect("bsp");
            let faces = bsp.lump(crate::emit::lump::FACES).expect("faces").into_owned();
            let (mut plain, mut displaced) = (0, 0);
            for f in faces.chunks_exact(56) {
                if i16::from_le_bytes([f[12], f[13]]) < 0 { plain += 1 } else { displaced += 1 }
            }
            let brushes = bsp.lump(crate::emit::lump::BRUSHES).expect("brushes").len() / 12;
            (plain, displaced, brushes)
        };
        let (plain, displaced, brushes) = counts(&map);
        let (flat_plain, flat_displaced, flat_brushes) = counts(&flat);
        assert!(displaced > 0 && flat_displaced == 0);
        assert_eq!(plain, flat_plain, "the other faces of a displaced brush are removed, never drawn");
        assert_eq!(brushes, flat_brushes, "a displaced brush is not solid: only its displacement collides");
    }

    #[test]
    fn a_displacement_with_a_bad_power_drops_its_brush_without_panicking() {
        let map = hp_vmf::parse(include_str!("disp/testdata/disp_golden.vmf")).expect("vmf");
        let at = map.world.solids.iter().position(|s| s.sides.iter().any(|x| x.disp.is_some())).expect("disp");
        let mut broken = map.clone();
        for side in &mut broken.world.solids[at].sides {
            if let Some(d) = side.disp.as_deref_mut() {
                d.power = 7;
            }
        }
        let mut removed = map.clone();
        removed.world.solids.remove(at);
        let counts = |m: &hp_vmf::Map| {
            let out = crate::compile(m, &Mats, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |_| {})
                .expect("compile");
            let bsp = hp_bsp::Bsp::from_vec(out.bsp).expect("bsp");
            let faces = bsp.lump(crate::emit::lump::FACES).expect("faces").into_owned();
            let plain = faces.chunks_exact(56).filter(|f| i16::from_le_bytes([f[12], f[13]]) < 0).count();
            (plain, faces.len() / 56)
        };
        assert_eq!(counts(&broken), counts(&removed), "nothing of the broken brush reaches the map");
    }
}
