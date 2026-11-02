use glam::{DVec3, Vec3A};

use super::data::{MapData, i32_at};
use super::output::{decode, median};
use super::surface::{inside, lit_faces};
use super::*;
use crate::materials::{MaterialFacts, MaterialSource, facts_by_name, surf};

pub(super) struct Mats;

impl MaterialSource for Mats {
    fn facts(&self, name: &str) -> MaterialFacts {
        let mut f = facts_by_name(name);
        if name.to_ascii_lowercase().contains("bump") {
            f.surf |= surf::BUMPLIGHT;
        }
        f.reflectivity = [0.5; 3];
        f
    }
}

pub(super) fn block(id: &mut u32, lo: DVec3, hi: DVec3, mat: &str, top_mat: &str) -> String {
    let p = |x: f64, y: f64, z: f64| format!("({x} {y} {z})");
    let (a, b) = (lo, hi);
    let sides = [
        (
            format!("{} {} {}", p(a.x, b.y, b.z), p(b.x, b.y, b.z), p(b.x, a.y, b.z)),
            "[1 0 0 0] 0.25",
            "[0 -1 0 0] 0.25",
            top_mat,
        ),
        (
            format!("{} {} {}", p(a.x, a.y, a.z), p(b.x, a.y, a.z), p(b.x, b.y, a.z)),
            "[1 0 0 0] 0.25",
            "[0 -1 0 0] 0.25",
            mat,
        ),
        (
            format!("{} {} {}", p(a.x, b.y, b.z), p(a.x, a.y, b.z), p(a.x, a.y, a.z)),
            "[0 1 0 0] 0.25",
            "[0 0 -1 0] 0.25",
            mat,
        ),
        (
            format!("{} {} {}", p(b.x, b.y, a.z), p(b.x, a.y, a.z), p(b.x, a.y, b.z)),
            "[0 1 0 0] 0.25",
            "[0 0 -1 0] 0.25",
            mat,
        ),
        (
            format!("{} {} {}", p(b.x, b.y, b.z), p(a.x, b.y, b.z), p(a.x, b.y, a.z)),
            "[1 0 0 0] 0.25",
            "[0 0 -1 0] 0.25",
            mat,
        ),
        (
            format!("{} {} {}", p(b.x, a.y, a.z), p(a.x, a.y, a.z), p(a.x, a.y, b.z)),
            "[1 0 0 0] 0.25",
            "[0 0 -1 0] 0.25",
            mat,
        ),
    ];
    *id += 1;
    let mut s = format!("\tsolid\n\t{{\n\t\t\"id\" \"{id}\"\n");
    for (plane, u, v, m) in sides {
        *id += 1;
        s += &format!(
            "\t\tside\n\t\t{{\n\t\t\t\"id\" \"{id}\"\n\t\t\t\"plane\" \"{plane}\"\n\t\t\t\"material\" \"{m}\"\n\t\t\t\"uaxis\" \"{u}\"\n\t\t\t\"vaxis\" \"{v}\"\n\t\t\t\"rotation\" \"0\"\n\t\t\t\"lightmapscale\" \"16\"\n\t\t\t\"smoothing_groups\" \"0\"\n\t\t}}\n"
        );
    }
    s + "\t}\n"
}

pub(super) fn room(floor_mat: &str, entities: &str) -> String {
    room_with(floor_mat, "DEV/DEV_MEASUREGENERIC01B", entities)
}

pub(super) fn room_with(floor_mat: &str, ceiling_mat: &str, entities: &str) -> String {
    let mut id = 1;
    let w = "DEV/DEV_MEASUREGENERIC01B";
    let mut solids = String::new();
    let t = 16.0;
    let (lo, hi) = (DVec3::ZERO, DVec3::new(512.0, 256.0, 256.0));
    solids +=
        &block(&mut id, DVec3::new(lo.x - t, lo.y - t, lo.z - t), DVec3::new(hi.x + t, hi.y + t, lo.z), w, floor_mat);
    solids += &block(
        &mut id,
        DVec3::new(lo.x - t, lo.y - t, hi.z),
        DVec3::new(hi.x + t, hi.y + t, hi.z + t),
        ceiling_mat,
        ceiling_mat,
    );
    solids += &block(&mut id, DVec3::new(lo.x - t, lo.y - t, lo.z), DVec3::new(lo.x, hi.y + t, hi.z), w, w);
    solids += &block(&mut id, DVec3::new(hi.x, lo.y - t, lo.z), DVec3::new(hi.x + t, hi.y + t, hi.z), w, w);
    solids += &block(&mut id, DVec3::new(lo.x, lo.y - t, lo.z), DVec3::new(hi.x, lo.y, hi.z), w, w);
    solids += &block(&mut id, DVec3::new(lo.x, hi.y, lo.z), DVec3::new(hi.x, hi.y + t, hi.z), w, w);
    solids += &block(&mut id, DVec3::new(240.0, 64.0, 0.0), DVec3::new(272.0, 192.0, 192.0), w, w);
    format!(
        "versioninfo\n{{\n\t\"editorversion\" \"400\"\n\t\"mapversion\" \"1\"\n}}\nworld\n{{\n\t\"id\" \"1\"\n\t\"mapversion\" \"1\"\n\t\"classname\" \"worldspawn\"\n\t\"skyname\" \"sky_day01_01\"\n{solids}}}\n{entities}\
         entity\n{{\n\t\"id\" \"9000\"\n\t\"classname\" \"info_player_start\"\n\t\"origin\" \"32 32 16\"\n}}\n"
    )
}

pub(super) fn light_entity(id: u32, origin: &str, extra: &str) -> String {
    format!(
        "entity\n{{\n\t\"id\" \"{id}\"\n\t\"classname\" \"light\"\n\t\"origin\" \"{origin}\"\n\t\"_light\" \"255 255 255 200\"\n\t\"_quadratic_attn\" \"1\"\n{extra}}}\n"
    )
}

pub(super) fn compile_vmf(text: &str) -> Vec<u8> {
    let map = hp_vmf::parse(text).expect("vmf");
    let out = crate::compile(&map, &Mats, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |_| {})
        .expect("compile");
    assert!(out.leak.is_none(), "the test room leaks");
    out.bsp
}

fn quick() -> RadOptions {
    RadOptions {
        hdr: true,
        ldr: true,
        bounces: 2,
        quality: RadQuality::Draft,
        threads: Some(2),
        ..RadOptions::default()
    }
}

pub(super) fn luxel_at(bsp: &[u8], p: DVec3, normal: DVec3, hdr: bool) -> Option<Vec3A> {
    let src = hp_bsp::Bsp::from_vec(bsp.to_vec()).ok()?;
    let map = MapData::read(&src).ok()?;
    let light = src.lump(if hdr { lump::LIGHTING_HDR } else { lump::LIGHTING }).ok()?.into_owned();
    let faces = src.lump(if hdr { lump::FACES_HDR } else { lump::FACES }).ok()?.into_owned();
    for f in lit_faces(&map, 45.0).into_iter().flatten() {
        if f.normal.dot(normal) < 0.99 || (p.dot(f.normal) - f.poly[0].dot(f.normal)).abs() > 0.5 {
            continue;
        }
        let c = f.luxel_coords(p);
        if !inside(&f.poly2, c) {
            continue;
        }
        let ofs = i32_at(&faces, f.face * 56 + 20);
        let (s, t) = ((c.x.round() as usize).min(f.w - 1), (c.y.round() as usize).min(f.h - 1));
        let at = ofs as usize + (t * f.w + s) * 4;
        let b = light.get(at..at + 4)?;
        return Some(decode([b[0], b[1], b[2], b[3]]));
    }
    None
}

#[test]
fn box_room_light_and_shadow() {
    let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
    let mut lines = Vec::new();
    let out = light_bsp(&bsp, &quick(), &mut |l| lines.push(l.to_owned())).expect("light");
    assert!(crate::validate::validate(&out).is_empty(), "{:?}", crate::validate::validate(&out));
    assert!(check_lighting(&out).is_empty(), "{:?}", check_lighting(&out));
    let floor = |x: f64, y: f64| luxel_at(&out, DVec3::new(x, y, 0.0), DVec3::Z, false).expect("floor luxel").x;
    let (near, far) = (floor(100.0, 40.0), floor(200.0, 40.0));
    assert!(near > far * 1.2, "near {near} far {far}");
    let below = floor(64.0, 128.0);
    assert!((150.0..300.0).contains(&below), "under the light: {below}");
    let (lit, shadow) = (floor(350.0, 10.0), floor(350.0, 128.0));
    assert!(shadow < lit * 0.5, "lit {lit} shadow {shadow}");
    assert!(shadow > 0.0, "bounced light reaches the shadow");
    let hdr = luxel_at(&out, DVec3::new(100.0, 40.0, 0.0), DVec3::Z, true).unwrap().x;
    assert!((hdr - near).abs() <= near * 0.02);
    let src = hp_bsp::Bsp::from_vec(out.clone()).unwrap();
    assert_eq!(src.lump(lump::LEAF_AMBIENT_INDEX).unwrap().len(), src.lump(lump::LEAFS).unwrap().len() / 32 * 4);
    assert!(!src.lump(lump::LEAF_AMBIENT_LIGHTING).unwrap().is_empty());
    assert_eq!(src.lump(15).unwrap().len(), 88);
    assert_eq!(src.lump(54).unwrap().len(), 88);
    assert!(lines.iter().any(|l| l.contains("direct light")), "{lines:?}");
}

#[test]
fn layout_round_trip() {
    let ents = light_entity(500, "64 128 128", "") + &light_entity(501, "400 128 200", "\t\"targetname\" \"lamp\"\n");
    let bsp = compile_vmf(&room("DEV/BUMP_FLOOR", &ents));
    let out = light_bsp(&bsp, &quick(), &mut |_| {}).expect("light");
    assert!(crate::validate::validate(&out).is_empty());
    assert!(check_lighting(&out).is_empty(), "{:?}", check_lighting(&out));
    let src = hp_bsp::Bsp::from_vec(out.clone()).unwrap();
    let map = MapData::read(&src).unwrap();
    assert!(map.entities.iter().any(|e| e.get("targetname") == Some("lamp") && e.get("style") == Some("32")));
    let light = src.lump(lump::LIGHTING).unwrap().into_owned();
    let faces = src.lump(lump::FACES).unwrap().into_owned();
    assert_eq!(faces, src.lump(lump::FACES_HDR).unwrap().into_owned());
    let lit = lit_faces(&map, 45.0);
    let mut end = 0usize;
    let (mut bumped, mut styled) = (0, 0);
    for (i, f) in lit.iter().enumerate() {
        let o = i * 56;
        let ofs = i32_at(&faces, o + 20);
        let styles: Vec<u8> = faces[o + 16..o + 20].iter().copied().take_while(|s| *s != 255).collect();
        let Some(f) = f else {
            assert_eq!(ofs, -1);
            assert!(styles.is_empty());
            continue;
        };
        assert_eq!(styles[0], 0);
        assert_eq!(ofs as usize, end + 4 * styles.len(), "face {i}");
        let nn = if f.bump { 4 } else { 1 };
        bumped += usize::from(f.bump);
        styled += usize::from(styles.contains(&32));
        let lux = f.luxels();
        for (k, _) in styles.iter().enumerate() {
            let flat: Vec<Vec3A> = (0..lux)
                .map(|j| {
                    let at = ofs as usize + (k * nn * lux + j) * 4;
                    decode([light[at], light[at + 1], light[at + 2], light[at + 3]])
                })
                .collect();
            let at = ofs as usize - 4 * (k + 1);
            let avg = decode([light[at], light[at + 1], light[at + 2], light[at + 3]]);
            let m = median(&flat);
            assert!((avg - m).abs().max_element() <= m.max_element() / 64.0 + 1e-6, "face {i} style {k}: {avg} vs {m}");
        }
        end = ofs as usize + lux * 4 * nn * styles.len();
    }
    assert_eq!(end, light.len());
    assert!(bumped > 0 && styled > 0, "bumped {bumped} styled {styled}");
    let p = DVec3::new(100.0, 40.0, 0.0);
    for f in lit.iter().flatten().filter(|f| f.bump) {
        let c = f.luxel_coords(p);
        if !inside(&f.poly2, c) {
            continue;
        }
        let ofs = i32_at(&faces, f.face * 56 + 20) as usize;
        let j = (c.y.round() as usize).min(f.h - 1) * f.w + (c.x.round() as usize).min(f.w - 1);
        let v: Vec<f32> = (0..4)
            .map(|n| {
                let at = ofs + (n * f.luxels() + j) * 4;
                decode([light[at], light[at + 1], light[at + 2], light[at + 3]]).x
            })
            .collect();
        assert!(v.iter().all(|x| *x > 0.0), "{v:?}");
    }
    let again = light_bsp(&out, &quick(), &mut |_| {}).unwrap();
    assert!(again == out, "relighting changed the file");
    let threads = light_bsp(&bsp, &RadOptions { threads: Some(5), ..quick() }, &mut |_| {}).unwrap();
    assert!(threads == out, "the thread count changed the result");
}

#[test]
fn bad_input_never_panics() {
    assert!(light_bsp(b"not a bsp", &quick(), &mut |_| {}).is_err());
    assert!(light_bsp(&[], &quick(), &mut |_| {}).is_err());
    assert!(light_bsp(&[0u8; 4], &RadOptions { hdr: false, ldr: false, ..quick() }, &mut |_| {}).is_err());
    let bsp = compile_vmf(&room(
        "DEV/BUMP_FLOOR",
        &light_entity(500, "64 128 128", "\t\"_fifty_percent_distance\" \"nan\"\n"),
    ));
    let src = hp_bsp::Bsp::from_vec(bsp.clone()).unwrap();
    let mut seed = 0x1234_5678u32;
    let mut rnd = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    for round in 0..24 {
        let mut b = bsp.clone();
        let target = [
            lump::FACES,
            lump::TEXINFO,
            lump::PLANES,
            lump::DISPINFO,
            lump::NODES,
            lump::LEAFS,
            lump::BRUSHSIDES,
            lump::VISIBILITY,
        ][round % 8];
        let r = src.lump_info(target).unwrap().range();
        if r.is_empty() {
            continue;
        }
        for _ in 0..16 {
            let at = r.start + rnd() as usize % r.len();
            b[at] = rnd() as u8;
        }
        let opts = RadOptions { bounces: 1, ..quick() };
        let _ = light_bsp(&b, &opts, &mut |_| {});
    }
}

fn valve_map(path: &str) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

pub(super) fn compare(ours: &[u8], valve: &[u8], hdr: bool, floor: f32) -> (Vec<f32>, usize) {
    let (fl, ll) = if hdr { (lump::FACES_HDR, lump::LIGHTING_HDR) } else { (lump::FACES, lump::LIGHTING) };
    let a = hp_bsp::Bsp::from_vec(ours.to_vec()).unwrap();
    let b = hp_bsp::Bsp::from_vec(valve.to_vec()).unwrap();
    let (fa, la) = (a.lump(fl).unwrap().into_owned(), a.lump(ll).unwrap().into_owned());
    let (fb, lb) = (b.lump(fl).unwrap().into_owned(), b.lump(ll).unwrap().into_owned());
    let mut ratios = Vec::new();
    let mut same_size = 0;
    for i in 0..fa.len().min(fb.len()) / 56 {
        let o = i * 56;
        let (oa, ob) = (i32_at(&fa, o + 20), i32_at(&fb, o + 20));
        if oa < 0 || ob < 0 || fa[o + 16] != 0 || fb[o + 16] != 0 {
            continue;
        }
        same_size += 1;
        let n = ((i32_at(&fa, o + 36) + 1) * (i32_at(&fa, o + 40) + 1)) as usize;
        for j in 0..n {
            let get = |l: &[u8], ofs: i32| {
                let at = ofs as usize + j * 4;
                l.get(at..at + 4).map(|c| decode([c[0], c[1], c[2], c[3]]))
            };
            if let (Some(x), Some(y)) = (get(&la, oa), get(&lb, ob))
                && y.max_element() > floor
                && !(y.y == 0.0 && y.z == 0.0 && y.x > 1000.0)
            {
                ratios.push((x.x + x.y + x.z) / (y.x + y.y + y.z));
            }
        }
    }
    ratios.sort_by(f32::total_cmp);
    (ratios, same_size)
}

fn summary(bsp: &[u8], hdr: bool) -> String {
    let b = hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap();
    let (fl, ll, il, al, wl) = if hdr {
        (lump::FACES_HDR, lump::LIGHTING_HDR, lump::LEAF_AMBIENT_INDEX_HDR, lump::LEAF_AMBIENT_LIGHTING_HDR, 54)
    } else {
        (lump::FACES, lump::LIGHTING, lump::LEAF_AMBIENT_INDEX, lump::LEAF_AMBIENT_LIGHTING, 15)
    };
    let faces = b.lump(fl).unwrap().into_owned();
    let light = b.lump(ll).unwrap().into_owned();
    let texinfo = b.lump(lump::TEXINFO).unwrap().into_owned();
    let mut styles = std::collections::BTreeMap::new();
    let mut bump_ratio = Vec::new();
    let mut flat = Vec::new();
    for i in 0..faces.len() / 56 {
        let o = i * 56;
        let ofs = i32_at(&faces, o + 20);
        let st: Vec<u8> = faces[o + 16..o + 20].iter().copied().take_while(|s| *s != 255).collect();
        *styles.entry(st.clone()).or_insert(0usize) += 1;
        if ofs < 0 || st.first() != Some(&0) {
            continue;
        }
        let ti = super::data::i16_at(&faces, o + 10).max(0) as usize;
        let bump = i32_at(&texinfo, ti * 72 + 64) as u32 & surf::BUMPLIGHT != 0;
        let n = ((i32_at(&faces, o + 36) + 1) * (i32_at(&faces, o + 40) + 1)) as usize;
        let get = |k: usize| {
            let at = ofs as usize + k * 4;
            light.get(at..at + 4).map_or(Vec3A::ZERO, |c| decode([c[0], c[1], c[2], c[3]]))
        };
        for j in 0..n {
            let f = get(j);
            let sum = f.x + f.y + f.z;
            flat.push(sum / 3.0);
            if bump && sum > 3.0 {
                let bs: f32 = (1..4).map(|k| get(k * n + j)).map(|v| v.x + v.y + v.z).sum();
                bump_ratio.push(bs / 3.0 / sum);
            }
        }
    }
    flat.sort_by(f32::total_cmp);
    bump_ratio.sort_by(f32::total_cmp);
    let index = b.lump(il).unwrap().into_owned();
    let amb = b.lump(al).unwrap().into_owned();
    let mut cubes: Vec<f32> = (0..amb.len() / 28)
        .flat_map(|s| (0..6).map(move |k| s * 28 + k * 4))
        .map(|at| decode([amb[at], amb[at + 1], amb[at + 2], amb[at + 3]]).max_element())
        .collect();
    cubes.sort_by(f32::total_cmp);
    let mut top: Vec<(Vec<u8>, usize)> = styles.into_iter().collect();
    top.sort_by_key(|b| std::cmp::Reverse(b.1));
    format!(
        "{} lightmap bytes, styles {:?}, flat luxels p25 {:.2} median {:.2} p75 {:.2}, bump/flat median {:.3} (p10 {:.3} p90 {:.3}), {} ambient samples in {} leaves (cube median {:.4}), {} world lights",
        light.len(),
        &top[..top.len().min(4)],
        pct(&flat, 0.25),
        pct(&flat, 0.5),
        pct(&flat, 0.75),
        pct(&bump_ratio, 0.5),
        pct(&bump_ratio, 0.1),
        pct(&bump_ratio, 0.9),
        amb.len() / 28,
        index.len() / 4,
        pct(&cubes, 0.5),
        b.lump(wl).unwrap().len() / 88
    )
}

pub(super) fn pct(v: &[f32], p: f64) -> f32 {
    v.get(((v.len() as f64 - 1.0) * p) as usize).copied().unwrap_or(f32::NAN)
}

#[test]
fn valve_test_map() {
    let Some(valve) =
        valve_map("C:/Users/oracle/Desktop/gmodmapping/test_maps/.valve_ref/hammerpro_test_areaportal.bsp")
    else {
        eprintln!("Valve's test map is not here: skipped");
        return;
    };
    let opts = RadOptions {
        hdr: false,
        ldr: true,
        bounces: 0,
        quality: RadQuality::Normal,
        threads: None,
        ..RadOptions::default()
    };
    let out = light_bsp(&valve, &opts, &mut |l| eprintln!("{l}")).expect("light");
    assert!(crate::validate::validate(&out).is_empty());
    assert!(check_lighting(&out).is_empty(), "{:?}", check_lighting(&out));
    let (r, faces) = compare(&out, &valve, false, 5.0);
    eprintln!(
        "test map: {faces} faces, {} luxels, ours/Valve p10 {:.3} p25 {:.3} median {:.3} p75 {:.3} p90 {:.3}",
        r.len(),
        pct(&r, 0.1),
        pct(&r, 0.25),
        pct(&r, 0.5),
        pct(&r, 0.75),
        pct(&r, 0.9)
    );
    assert!(r.len() > 1000);
    assert!(
        (0.97..1.03).contains(&pct(&r, 0.5)) && pct(&r, 0.1) > 0.9 && pct(&r, 0.9) < 1.1,
        "median {}",
        pct(&r, 0.5)
    );
    let a = hp_bsp::Bsp::from_vec(out).unwrap();
    let b = hp_bsp::Bsp::from_vec(valve).unwrap();
    let (wa, wb) = (a.lump(15).unwrap().into_owned(), b.lump(15).unwrap().into_owned());
    assert_eq!(wa.len(), wb.len());
    for k in 0..wa.len() / 4 {
        let (x, y) = (super::data::f32_at(&wa, k * 4), super::data::f32_at(&wb, k * 4));
        assert!((x - y).abs() <= 1e-3 * y.abs().max(1.0), "world light word {k}: {x} vs {y}");
    }
}

fn relight_and_compare(path: &str) {
    let Some(valve) = valve_map(path) else {
        eprintln!("{path} is not here: skipped");
        return;
    };
    let bounces: u32 = std::env::var("RAD_BOUNCES").ok().and_then(|v| v.parse().ok()).unwrap_or(100);
    let qualities = if std::env::var("RAD_DRAFT_ONLY").is_ok() {
        vec![RadQuality::Draft]
    } else {
        vec![RadQuality::Draft, RadQuality::Normal]
    };
    let has_hdr = hp_bsp::Bsp::from_vec(valve.clone()).unwrap().lump_info(lump::LIGHTING_HDR).is_ok_and(|l| l.len > 0);
    for quality in qualities {
        let t = std::time::Instant::now();
        let opts = RadOptions { quality, bounces, ..RadOptions::default() };
        let out = light_bsp(&valve, &opts, &mut |l| eprintln!("  {l}")).expect("light");
        eprintln!("{path} {quality:?}: {:.1} s", t.elapsed().as_secs_f64());
        if let Ok(save) = std::env::var("RAD_SAVE") {
            std::fs::write(format!("{save}_{quality:?}.bsp"), &out).unwrap();
        }
        assert!(check_lighting(&out).is_empty(), "{:?}", check_lighting(&out));
        assert_eq!(crate::validate::validate(&out), crate::validate::validate(&valve));
        for hdr in [false, true] {
            if hdr && !has_hdr {
                continue;
            }
            let (r, faces) = compare(&out, &valve, hdr, 2.0);
            eprintln!(
                "  {}: {faces} faces, {} luxels, ours/Valve p10 {:.3} p25 {:.3} median {:.3} p75 {:.3} p90 {:.3}",
                if hdr { "HDR" } else { "LDR" },
                r.len(),
                pct(&r, 0.1),
                pct(&r, 0.25),
                pct(&r, 0.5),
                pct(&r, 0.75),
                pct(&r, 0.9)
            );
            eprintln!("    ours:  {}", summary(&out, hdr));
            eprintln!("    Valve: {}", summary(&valve, hdr));
        }
    }
}

#[test]
#[ignore]
fn gm_construct() {
    relight_and_compare("D:/SteamLibrary/steamapps/common/GarrysMod/garrysmod/maps/gm_construct.bsp");
}

#[test]
#[ignore]
fn big_map() {
    let path = std::env::var("RAD_MAP").unwrap_or_else(|_| {
        "D:/SteamLibrary/steamapps/common/GarrysMod/garrysmod/maps/union_blue_lock_v2.bsp".to_owned()
    });
    relight_and_compare(&path);
}

#[test]
fn own_compiler_test_maps() {
    let dir = "C:/Users/oracle/Desktop/gmodmapping/test_maps";
    let Ok(list) = std::fs::read_dir(dir) else {
        eprintln!("{dir} is not here: skipped");
        return;
    };
    let mut names: Vec<std::path::PathBuf> =
        list.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "vmf")).collect();
    names.sort();
    for path in names {
        let text = std::fs::read_to_string(&path).unwrap();
        let Ok(map) = hp_vmf::parse(&text) else { continue };
        let Ok(out) =
            crate::compile(&map, &Mats, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |_| {})
        else {
            eprintln!("{} does not compile: skipped", path.display());
            continue;
        };
        let opts = RadOptions { bounces: 0, ..RadOptions::default() };
        let lit = light_bsp(&out.bsp, &opts, &mut |_| {}).expect("light");
        assert!(
            crate::validate::validate(&lit).is_empty(),
            "{}: {:?}",
            path.display(),
            crate::validate::validate(&lit)
        );
        assert!(check_lighting(&lit).is_empty(), "{}: {:?}", path.display(), check_lighting(&lit));
        if let Ok(save) = std::env::var("RAD_SAVE") {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            std::fs::write(format!("{save}_{name}.bsp"), &lit).unwrap();
        }
        let valve_path = format!("{dir}/.valve_ref/{}.bsp", path.file_stem().unwrap().to_string_lossy());
        let Some(valve) = valve_map(&valve_path) else { continue };
        let mut ratios = Vec::new();
        for x in (-500..520).step_by(24) {
            for y in (-250..256).step_by(24) {
                let p = DVec3::new(f64::from(x), f64::from(y), 0.0);
                if let (Some(a), Some(b)) = (luxel_at(&lit, p, DVec3::Z, false), luxel_at(&valve, p, DVec3::Z, false))
                    && b.x > 5.0
                    && !(b.y == 0.0 && b.z == 0.0)
                {
                    ratios.push(a.x / b.x);
                }
            }
        }
        ratios.sort_by(f32::total_cmp);
        eprintln!(
            "{}: {} floor points, ours/Valve p10 {:.3} median {:.3} p90 {:.3}",
            path.display(),
            ratios.len(),
            pct(&ratios, 0.1),
            pct(&ratios, 0.5),
            pct(&ratios, 0.9)
        );
        if ratios.len() > 50 {
            assert!((0.9..1.1).contains(&pct(&ratios, 0.5)), "median {}", pct(&ratios, 0.5));
        }
    }
}

#[test]
fn detail_props_take_high_styles() {
    let lights = light_entity(500, "64 128 128", "")
        + &light_entity(
            501,
            "448 128 128",
            "	\"style\" \"100\"
",
        );
    let map = hp_vmf::parse(&room("NATURE/GRASS", &lights)).expect("vmf");
    let out = crate::compile(&map, &GrassMats, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |_| {})
        .expect("compile");
    let lit = light_bsp(&out.bsp, &quick(), &mut |_| {}).expect("light");
    let b = hp_bsp::Bsp::from_vec(lit).unwrap();
    let g = b.game_lumps().unwrap();
    let records =
        crate::detail_props::parse_styles(&g.get(crate::detail_props::DPLT).expect("dplt").data().unwrap()).unwrap();
    assert!(
        records.iter().any(|(c, s)| *s == 100 && decode(*c).max_element() > 0.0),
        "{:?}",
        &records[..records.len().min(8)]
    );
}

#[test]
fn split_floor_has_no_seams() {
    let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
    let opts = RadOptions {
        hdr: false,
        ldr: true,
        bounces: 0,
        quality: RadQuality::Normal,
        threads: Some(2),
        ..RadOptions::default()
    };
    let lit = light_bsp(&bsp, &opts, &mut |_| {}).expect("light");
    let b = hp_bsp::Bsp::from_vec(lit).unwrap();
    let (faces, light) = (b.lump(lump::FACES).unwrap().into_owned(), b.lump(lump::LIGHTING).unwrap().into_owned());
    let map = MapData::read(&b).unwrap();
    let mut cells: std::collections::HashMap<(i32, i32), Vec<[u8; 4]>> = std::collections::HashMap::new();
    let mut floors = 0;
    for (i, f) in map.faces.iter().enumerate() {
        let o = i * 56;
        let ofs = i32_at(&faces, o + 20);
        let up = map.face_plane(f).is_some_and(|p| p.normal.z > 0.99 && p.dist.abs() < 0.01);
        if ofs < 0 || !up {
            continue;
        }
        floors += 1;
        let (mx, my) = (i32_at(&faces, o + 28), i32_at(&faces, o + 32));
        let (w, h) = (i32_at(&faces, o + 36) + 1, i32_at(&faces, o + 40) + 1);
        for t in 0..h {
            for s in 0..w {
                let at = ofs as usize + ((t * w + s) * 4) as usize;
                let c = [light[at], light[at + 1], light[at + 2], light[at + 3]];
                cells.entry((mx + s, my + t)).or_default().push(c);
            }
        }
    }
    assert!(floors > 1, "the pillar splits the floor");
    let shared: Vec<&Vec<[u8; 4]>> =
        cells.values().filter(|v| v.len() > 1 && v.iter().all(|c| decode(*c).max_element() > 0.0)).collect();
    assert!(!shared.is_empty());
    for v in shared {
        assert!(v.iter().all(|c| *c == v[0]), "{v:?}");
    }
}

#[test]
fn phong_normals_blend_to_the_vertices() {
    let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
    let src = hp_bsp::Bsp::from_vec(bsp).unwrap();
    let map = MapData::read(&src).unwrap();
    let mut f = lit_faces(&map, 45.0).into_iter().flatten().find(|f| f.normal.z > 0.99 && f.poly.len() >= 3).unwrap();
    assert!(f.vnormals.is_none());
    let tilt = DVec3::new(0.3, 0.0, 1.0).normalize();
    f.vnormals = Some(vec![tilt; f.poly.len()]);
    let center = f.poly.iter().copied().sum::<DVec3>() / f.poly.len() as f64;
    assert!((f.phong(center) - f.normal).length() < 1e-6);
    let near = f.poly[0] * 0.999 + center * 0.001;
    assert!((f.phong(near) - tilt).length() < 0.01, "{:?}", f.phong(near));
    let mid = f.phong(f.poly[0] * 0.5 + center * 0.5);
    assert!(mid.x > 0.0 && mid.x < tilt.x);
}

pub(super) struct GrassMats;

impl MaterialSource for GrassMats {
    fn facts(&self, name: &str) -> MaterialFacts {
        let mut f = Mats.facts(name);
        if name.to_ascii_lowercase().contains("grass") {
            f.detail_type = Some("testgrass".to_owned());
        }
        f
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        path.eq_ignore_ascii_case("detail.vbsp").then(|| {
            b"detail { testgrass { \"density\" \"2000\" g { \"alpha\" \"1\" a { \"sprite\" \"0 0 64 64 512\" \"spritesize\" \"0.5 0 16 16\" \"amount\" \"1\" \"upright\" \"1\" } } } }"
                .to_vec()
        })
    }
}

#[test]
fn detail_props_lit() {
    let lights =
        light_entity(500, "64 128 128", "") + &light_entity(501, "448 128 128", "\t\"targetname\" \"switch\"\n");
    let map = hp_vmf::parse(&room("NATURE/GRASS", &lights)).expect("vmf");
    let mut log = Vec::new();
    let out = crate::compile(&map, &GrassMats, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |l| {
        log.push(l.to_owned())
    })
    .expect("compile");
    let dprp = |bsp: &[u8]| {
        let b = hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap();
        let g = b.game_lumps().unwrap();
        let l = g.get(crate::detail_props::DPRP).expect("dprp");
        assert_eq!(l.version, 4);
        let styles: Vec<usize> = [crate::detail_props::DPLT, crate::detail_props::DPLH]
            .iter()
            .filter_map(|id| g.get(*id).map(|s| crate::detail_props::parse_styles(&s.data().unwrap()).unwrap().len()))
            .collect();
        (crate::detail_props::DetailLump::parse(&l.data().unwrap()).unwrap(), styles)
    };
    let (placed, styles) = dprp(&out.bsp);
    assert!((150..400).contains(&placed.objects.len()), "{} {log:?}", placed.objects.len());
    assert!(
        placed.objects.iter().all(|o| o.lighting == [255, 255, 255, 0]
            && o.origin.z == 0.0
            && o.kind == crate::detail_props::kind::SPRITE)
    );
    assert!(styles.is_empty());
    let lit = light_bsp(&out.bsp, &quick(), &mut |_| {}).expect("light");
    assert!(crate::validate::validate(&lit).is_empty(), "{:?}", crate::validate::validate(&lit));
    let (done, styles) = dprp(&lit);
    assert_eq!(styles.len(), 2);
    assert_eq!(done.objects.len(), placed.objects.len());
    let value = |c: [u8; 4]| decode(c).max_element();
    let near = done.objects.iter().filter(|o| o.origin.x < 128.0).map(|o| value(o.lighting)).fold(0.0f32, f32::max);
    let far = done.objects.iter().filter(|o| o.origin.x > 384.0).map(|o| value(o.lighting)).fold(f32::MAX, f32::min);
    assert!(near > 20.0 && near > far, "near {near} far {far}");
    let styled = done.objects.iter().filter(|o| o.light_style_count > 0).count();
    assert!(styled > done.objects.len() / 2 && styles[0] >= styled, "{styled} {styles:?}");
}

fn leaf_flags(bsp: &[u8]) -> Vec<(u32, u16)> {
    let src = hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap();
    let leafs = src.lump(lump::LEAFS).unwrap().into_owned();
    (0..leafs.len() / 32)
        .map(|i| {
            (
                u32::from_le_bytes(leafs[i * 32..i * 32 + 4].try_into().unwrap()),
                u16::from_le_bytes([leafs[i * 32 + 6], leafs[i * 32 + 7]]) >> 9,
            )
        })
        .collect()
}

#[test]
fn leaf_sky_flags_like_vrad() {
    let sun = "entity\n{\n\t\"id\" \"600\"\n\t\"classname\" \"light_environment\"\n\t\"origin\" \"64 64 64\"\n\t\"_light\" \"255 255 255 200\"\n\t\"_ambient\" \"50 50 50 20\"\n\t\"pitch\" \"-60\"\n\t\"angles\" \"0 30 0\"\n}\n";
    let opts = RadOptions { bounces: 0, ..quick() };
    for (sky, flag) in [("TOOLS/TOOLSSKYBOX", 1u16), ("TOOLS/TOOLSSKYBOX2D", 4)] {
        let lit = light_bsp(&compile_vmf(&room_with("DEV/DEV_MEASUREGENERIC01B", sky, sun)), &opts, &mut |_| {})
            .expect("light");
        let flags = leaf_flags(&lit);
        assert_eq!(flags[0].1, 0);
        for (contents, f) in &flags[1..] {
            let want = if contents & 1 != 0 { 0 } else { flag };
            assert_eq!(*f, want, "{sky}: leaf contents {contents:#x}");
        }
    }
    for (sky, flag) in [("TOOLS/TOOLSSKYBOX", 1u16), ("TOOLS/TOOLSSKYBOX2D", 4)] {
        let text = room_with("DEV/DEV_MEASUREGENERIC01B", sky, &light_entity(500, "64 128 128", ""));
        let lit = light_bsp(&compile_vmf(&text), &opts, &mut |_| {}).expect("light");
        let flags = leaf_flags(&lit);
        assert_eq!(flags[0].1, 0);
        for (contents, f) in &flags[1..] {
            let want = if contents & 1 != 0 { 0 } else { flag };
            assert_eq!(*f, want, "no sun, {sky}: leaf contents {contents:#x}");
        }
    }
}

fn sky_rooms(extra: &str) -> String {
    let mut id = 1;
    let w = "DEV/DEV_MEASUREGENERIC01B";
    let (sky, sky2d) = ("TOOLS/TOOLSSKYBOX", "TOOLS/TOOLSSKYBOX2D");
    let b = |id: &mut u32, lo: [f64; 3], hi: [f64; 3], m: &str| {
        block(id, DVec3::from_array(lo), DVec3::from_array(hi), m, m)
    };
    let mut s = String::new();
    s += &b(&mut id, [-16.0, -16.0, -16.0], [528.0, 1552.0, 0.0], w);
    s += &b(&mut id, [-16.0, -16.0, 256.0], [528.0, 512.0, 272.0], sky);
    s += &b(&mut id, [-16.0, 1024.0, 256.0], [528.0, 1552.0, 272.0], sky2d);
    s += &b(&mut id, [-16.0, 512.0, 128.0], [528.0, 1024.0, 272.0], w);
    s += &b(&mut id, [-16.0, 512.0, 0.0], [448.0, 768.0, 128.0], w);
    s += &b(&mut id, [512.0, 512.0, 0.0], [528.0, 1024.0, 128.0], w);
    s += &b(&mut id, [-16.0, 768.0, 0.0], [0.0, 1024.0, 128.0], w);
    s += &b(&mut id, [64.0, 832.0, 0.0], [512.0, 1024.0, 128.0], w);
    s += &b(&mut id, [-16.0, -16.0, 0.0], [0.0, 512.0, 256.0], w);
    s += &b(&mut id, [512.0, -16.0, 0.0], [528.0, 512.0, 256.0], w);
    s += &b(&mut id, [0.0, -16.0, 0.0], [512.0, 0.0, 256.0], w);
    s += &b(&mut id, [-16.0, 1024.0, 0.0], [0.0, 1552.0, 256.0], w);
    s += &b(&mut id, [512.0, 1024.0, 0.0], [528.0, 1552.0, 256.0], w);
    s += &b(&mut id, [0.0, 1536.0, 0.0], [512.0, 1552.0, 256.0], w);
    let sun = "entity
{
	\"id\" \"600\"
	\"classname\" \"light_environment\"
	\"origin\" \"64 64 64\"
	\"_light\" \"255 255 255 200\"
	\"_ambient\" \"50 50 50 20\"
	\"pitch\" \"-60\"
	\"angles\" \"0 30 0\"
}
";
    format!(
        "versioninfo
{{
	\"editorversion\" \"400\"
	\"mapversion\" \"1\"
}}
world
{{
	\"id\" \"1\"
	\"mapversion\" \"1\"
	\"classname\" \"worldspawn\"
	\"skyname\" \"sky_day01_01\"
{s}}}
{sun}{extra}         entity
{{
	\"id\" \"9000\"
	\"classname\" \"info_player_start\"
	\"origin\" \"256 256 16\"
}}
"
    )
}

fn sky_flags_at(bsp: &[u8], points: &[DVec3]) -> Vec<u16> {
    let src = hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap();
    let map = MapData::read(&src).unwrap();
    let flags = leaf_flags(bsp);
    points
        .iter()
        .map(|p| flags[map.point_leaf(map.models[0].head, *p).unwrap()].1 & (LEAF_FLAGS_SKY | LEAF_FLAGS_SKY2D))
        .collect()
}

#[test]
fn leaf_sky_flags_per_cluster() {
    let opts = RadOptions { bounces: 0, ..quick() };
    let fog = "entity
{
	\"id\" \"700\"
	\"classname\" \"env_fog_controller\"
	\"origin\" \"256 256 64\"
	\"farz\" \"100000\"
}
";
    let points = [
        DVec3::new(256.0, 256.0, 64.0),
        DVec3::new(480.0, 600.0, 64.0),
        DVec3::new(32.0, 960.0, 64.0),
        DVec3::new(256.0, 1280.0, 64.0),
    ];
    for (extra, vis) in [("", crate::VisLevel::Full), (fog, crate::VisLevel::Full), ("", crate::VisLevel::Fast)] {
        let map = hp_vmf::parse(&sky_rooms(extra)).expect("vmf");
        let copts = crate::CompileOptions { vis, ..Default::default() };
        let out = crate::compile(&map, &Mats, &crate::props::NoModels, &copts, &mut |_| {}).expect("compile");
        assert!(out.leak.is_none());
        let lit = light_bsp(&out.bsp, &opts, &mut |_| {}).expect("light");
        let radial = leaf_flags(&lit).iter().filter(|(c, f)| c & 1 == 0 && f & LEAF_FLAGS_RADIAL != 0).count();
        assert_eq!(radial > 0, !extra.is_empty());
        let want = [LEAF_FLAGS_SKY, LEAF_FLAGS_SKY, LEAF_FLAGS_SKY2D, LEAF_FLAGS_SKY2D];
        assert_eq!(sky_flags_at(&lit, &points), want, "fog: {}, {vis:?}", !extra.is_empty());
    }
}

#[test]
fn displacements_without_shadows() {
    let w = "DEV/DEV_MEASUREGENERIC01B";
    let mut id = 700;
    let shelf = block(&mut id, DVec3::new(16.0, 80.0, 100.0), DVec3::new(112.0, 176.0, 108.0), w, w);
    let base = room(w, &light_entity(500, "64 128 200", ""));
    let at = base.find("\tsolid\n").expect("a solid");
    let text = format!("{}{shelf}{}", &base[..at], &base[at..]);
    let make = |shadows: bool| {
        let mut map = hp_vmf::parse(&text).expect("vmf");
        let solid = map
            .world
            .solids
            .iter_mut()
            .find(|s| s.sides.iter().any(|d| d.plane.iter().all(|p| p.z == 108.0)))
            .expect("shelf");
        let top = solid.sides.iter_mut().find(|d| d.plane.iter().all(|p| p.z == 108.0)).expect("top");
        let mut disp = hp_vmf::DispInfo::new_flat(2, DVec3::new(16.0, 80.0, 108.0), DVec3::Z);
        disp.set_casts_shadows(shadows);
        top.disp = Some(Box::new(disp));
        let out = crate::compile(&map, &Mats, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |_| {})
            .expect("compile");
        out.bsp
    };
    let floor = |bsp: &[u8]| luxel_at(bsp, DVec3::new(64.0, 128.0, 0.0), DVec3::Z, false).expect("floor").x;
    let top = |bsp: &[u8]| luxel_at(bsp, DVec3::new(64.0, 128.0, 108.0), DVec3::Z, false).expect("shelf").x;
    let opts = RadOptions { bounces: 0, ..quick() };
    let with = make(true);
    let without = make(false);
    let marked = |bsp: &[u8]| {
        let src = hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap();
        let d = src.lump(lump::DISPINFO).unwrap().into_owned();
        (0..d.len() / 176).any(|i| i32_at(&d, i * 176 + 24) & crate::disp::NO_SHADOW_BIT != 0)
    };
    assert!(!marked(&with) && marked(&without));
    let lit_with = light_bsp(&with, &opts, &mut |_| {}).expect("light");
    let mut lines = Vec::new();
    let lit_without = light_bsp(&without, &opts, &mut |l| lines.push(l.to_owned())).expect("light");
    assert!(!marked(&lit_without), "the game never sees the bit");
    assert!(lines.iter().any(|l| l.contains("cast no shadow")), "{lines:?}");
    let (dark, bright) = (floor(&lit_with), floor(&lit_without));
    assert!(dark < bright * 0.5, "shadowed {dark}, without shadows {bright}");
    assert!(top(&lit_without) > 0.0, "the displacement is still lit");
    let all = light_bsp(&with, &RadOptions { no_disp_shadows: true, ..opts }, &mut |_| {}).expect("light");
    assert!(floor(&all) > dark * 2.0, "-nodispshadows: {} vs {dark}", floor(&all));
}

#[test]
fn maximum_distance_is_for_the_engine_only() {
    let opts = RadOptions { bounces: 0, ..quick() };
    let lit = |extra: &str| {
        let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "40 20 128", extra)));
        light_bsp(&bsp, &opts, &mut |_| {}).expect("light")
    };
    let floor = |bsp: &[u8], x: f64| luxel_at(bsp, DVec3::new(x, 20.0, 0.0), DVec3::Z, false).expect("floor luxel").x;
    let (free, cut) = (lit(""), lit("\t\"_distance\" \"256\"\n"));
    assert!(floor(&free, 419.0) > 1.0, "the far floor is lit: {}", floor(&free, 419.0));
    for x in [194.0, 419.0] {
        assert_eq!(floor(&cut, x), floor(&free, x), "{x} units: VRAD lights past _distance");
    }
    let src = hp_bsp::Bsp::from_vec(cut).unwrap();
    let wl = src.lump(15).unwrap().into_owned();
    assert_eq!(super::data::f32_at(&wl, 60), 256.0, "the world light keeps the radius for the engine");
}

#[test]
fn no_physics_displacements_have_no_collision_mesh() {
    let w = "DEV/DEV_MEASUREGENERIC01B";
    let mut id = 700;
    let shelf = block(&mut id, DVec3::new(16.0, 80.0, 100.0), DVec3::new(112.0, 176.0, 108.0), w, w);
    let base = room(w, &light_entity(500, "64 128 200", ""));
    let at = base.find("\tsolid\n").expect("a solid");
    let text = format!("{}{shelf}{}", &base[..at], &base[at..]);
    let sizes = |flags: i32| {
        let mut map = hp_vmf::parse(&text).expect("vmf");
        let solid = map
            .world
            .solids
            .iter_mut()
            .find(|s| s.sides.iter().any(|d| d.plane.iter().all(|p| p.z == 108.0)))
            .expect("shelf");
        let top = solid.sides.iter_mut().find(|d| d.plane.iter().all(|p| p.z == 108.0)).expect("top");
        let mut disp = hp_vmf::DispInfo::new_flat(2, DVec3::new(16.0, 80.0, 108.0), DVec3::Z);
        disp.flags = flags;
        top.disp = Some(Box::new(disp));
        let out = crate::compile(&map, &Mats, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |_| {})
            .expect("compile");
        let src = hp_bsp::Bsp::from_vec(out.bsp).unwrap();
        let d = src.lump(lump::PHYSDISP).unwrap().into_owned();
        let info = src.lump(lump::DISPINFO).unwrap().into_owned();
        let count = u16::from_le_bytes([d[0], d[1]]) as usize;
        let sizes: Vec<u16> = (0..count).map(|i| u16::from_le_bytes([d[2 + i * 2], d[3 + i * 2]])).collect();
        (sizes, i32_at(&info, 24))
    };
    let (normal, tess) = sizes(0);
    assert_eq!(normal.len(), 1);
    assert_ne!(normal[0], 0xFFFF, "a collision mesh");
    assert_eq!(tess, i32::MIN);
    let (none, tess) = sizes(hp_geom::DISP_FLAG_NO_PHYSICS | hp_geom::DISP_FLAG_NO_RAY);
    assert_eq!(none, vec![0xFFFF], "no physics: no collision mesh");
    assert_eq!(tess, i32::MIN | hp_geom::DISP_FLAG_NO_PHYSICS | hp_geom::DISP_FLAG_NO_RAY, "the flags reach the game");
}

fn sun_yard(sun_keys: &[(&str, &str)]) -> Vec<u8> {
    let mut id = 1;
    let (w, sky) = ("DEV/DEV_MEASUREGENERIC01B", "TOOLS/TOOLSSKYBOX");
    let b = |id: &mut u32, lo: [f64; 3], hi: [f64; 3], m: &str| {
        block(id, DVec3::from_array(lo), DVec3::from_array(hi), m, m)
    };
    let mut s = String::new();
    s += &b(&mut id, [-528.0, -528.0, -16.0], [528.0, 528.0, 0.0], w);
    s += &b(&mut id, [-528.0, -528.0, 512.0], [528.0, 528.0, 528.0], sky);
    s += &b(&mut id, [-528.0, -528.0, 0.0], [-512.0, 528.0, 512.0], sky);
    s += &b(&mut id, [512.0, -528.0, 0.0], [528.0, 528.0, 512.0], sky);
    s += &b(&mut id, [-512.0, -528.0, 0.0], [512.0, -512.0, 512.0], sky);
    s += &b(&mut id, [-512.0, 512.0, 0.0], [512.0, 528.0, 512.0], sky);
    s += &b(&mut id, [-16.0, -16.0, 0.0], [16.0, 16.0, 128.0], w);
    let keys: String = sun_keys.iter().map(|(k, v)| format!("\t\"{k}\" \"{v}\"\n")).collect();
    let text = format!(
        "versioninfo\n{{\n\t\"editorversion\" \"400\"\n}}\nworld\n{{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n\t\"skyname\" \"sky_day01_01\"\n{s}}}\n\
         entity\n{{\n\t\"id\" \"600\"\n\t\"classname\" \"light_environment\"\n\t\"origin\" \"200 200 200\"\n\t\"_light\" \"255 255 255 300\"\n\t\"_ambient\" \"0 0 0 0\"\n{keys}}}\n\
         entity\n{{\n\t\"id\" \"601\"\n\t\"classname\" \"info_player_start\"\n\t\"origin\" \"256 256 16\"\n}}\n"
    );
    let bsp = compile_vmf(&text);
    light_bsp(&bsp, &RadOptions { bounces: 0, ..quick() }, &mut |_| {}).expect("light")
}

#[test]
fn sun_direction_from_angles_and_pitch() {
    let floor = |bsp: &[u8], x: f64, y: f64| luxel_at(bsp, DVec3::new(x, y, 0.0), DVec3::Z, false).expect("floor").x;
    let yaw0 = sun_yard(&[("angles", "-45 0 0")]);
    let (behind, before) = (floor(&yaw0, 80.0, 0.0), floor(&yaw0, -80.0, 0.0));
    assert!(behind < before * 0.2, "the shadow falls along +x: {behind} vs {before}");
    assert!(floor(&yaw0, 0.0, 80.0) > before * 0.8, "no shadow along +y");
    let pitch_key = sun_yard(&[("angles", "-10 90 0"), ("pitch", "-45")]);
    assert!(floor(&pitch_key, 0.0, 80.0) < floor(&pitch_key, 0.0, -80.0) * 0.2, "yaw 90 casts along +y");
    let long = sun_yard(&[("angles", "-10 90 0")]);
    let at = |bsp: &[u8]| floor(bsp, 0.0, 300.0);
    assert!(at(&long) < at(&pitch_key) * 0.2, "a low sun casts far: {} vs {}", at(&long), at(&pitch_key));
    let zero = sun_yard(&[("angles", "-10 90 0"), ("pitch", "0")]);
    assert!((at(&zero) - at(&long)).abs() <= at(&long).max(1.0) * 0.05, "pitch 0 keeps the pitch of angles");
    let lit = before;
    let expected = 300.0 * (45f32.to_radians()).sin();
    assert!((lit - expected).abs() < expected * 0.1, "cosine of the sun on the floor: {lit} vs {expected}");
}

#[test]
fn degenerate_lights_finish() {
    let ent = |pairs: &[(&str, &str)]| hp_bsp::Entity {
        pairs: pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
    };
    let base = compile_vmf(&room("DEV/BUMP_FLOOR", &light_entity(500, "64 128 128", "")));
    let src = hp_bsp::Bsp::from_vec(base).unwrap();
    let mut ents = src.entities().unwrap();
    ents.extend([
        ent(&[("classname", "light"), ("origin", "1e30 0 0"), ("_light", "inf inf inf 1e30")]),
        ent(&[
            ("classname", "light"),
            ("origin", "64 128 128"),
            ("_light", "255 255 255 200"),
            ("_quadratic_attn", "-5"),
            ("_fifty_percent_distance", "0.0001"),
            ("_zero_percent_distance", "1e-9"),
        ]),
        ent(&[("classname", "light"), ("origin", "nan nan nan"), ("_light", "255 255 255 200")]),
        ent(&[
            ("classname", "light"),
            ("origin", "250 128 100"),
            ("_light", "1e38 1e38 1e38 1e38"),
            ("_distance", "1e30"),
        ]),
        ent(&[
            ("classname", "light_spot"),
            ("origin", "300 100 100"),
            ("_light", "255 255 255 200"),
            ("_cone", "nan"),
            ("_inner_cone", "-1e9"),
            ("_exponent", "1e30"),
            ("pitch", "nan"),
        ]),
        ent(&[
            ("classname", "light_environment"),
            ("origin", "64 64 64"),
            ("_light", "255 255 255 1e9"),
            ("angles", "1e9 inf 0"),
            ("pitch", "nan"),
            ("SunSpreadAngle", "1e9"),
        ]),
    ]);
    let mut edit = src.edit();
    edit.set_entities(&ents).unwrap();
    let bsp = edit.to_vec().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let out = light_bsp(&bsp, &RadOptions { bounces: 3, ..quick() }, &mut |_| {});
        let _ = tx.send(out.map(|o| check_lighting(&o)));
    });
    let out = rx.recv_timeout(std::time::Duration::from_secs(120)).expect("the lighting of degenerate lights hangs");
    let problems = out.expect("light");
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn lighting_reports_its_stages() {
    let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
    let mut lines = Vec::new();
    light_bsp(&bsp, &quick(), &mut |l| lines.push(l.to_owned())).expect("light");
    let plan = lines.iter().find(|l| l.starts_with("Lighting stages: ")).expect("the stage plan");
    assert_eq!(plan, "Lighting stages: direct lighting, bounce rays, bounced light, leaf ambient, writing");
    let done = lines.iter().position(|l| l.starts_with("lighting: ")).expect("done line");
    assert!(lines[done..].iter().all(|l| !l.starts_with("Lighting ")), "no progress line after the end");
}

#[test]
fn lightmaps_only_fail_past_the_face_offset() {
    let most = hp_bsp::limits::LIGHTING.max();
    assert!(lightmap_room(81_214_012, most).is_ok(), "Valve's kasty: 81 MB of lightmaps load in Garry's Mod");
    assert!(lightmap_room(most, most).is_ok());
    let e = lightmap_room(most + 1, most).unwrap_err();
    assert!(e.contains("cannot load"), "{e}");
}
