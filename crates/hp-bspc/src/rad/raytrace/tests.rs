use glam::DVec3;

use super::super::tests::{block, compare, compile_vmf, light_entity, luxel_at, pct, quick, room};
use super::super::*;
use super::{DevicePreference, Effects, Quality, RayTracing};
use glam::Vec3A;

fn rt(device: DevicePreference) -> RadOptions {
    RadOptions {
        raytracing: Some(RayTracing {
            device,
            min_samples: Some(32),
            max_samples: Some(64),
            effects: Effects::none(),
            ..RayTracing::with_quality(Quality::Draft)
        }),
        ..quick()
    }
}

fn floor(bsp: &[u8], x: f64, y: f64) -> f32 {
    luxel_at(bsp, DVec3::new(x, y, 0.0), DVec3::Z, false).unwrap_or_else(|| panic!("no floor luxel at {x} {y}")).x
}

#[test]
fn ray_traced_room_matches_vrad_brightness_and_shadows() {
    let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
    let mut lines = Vec::new();
    let out = light_bsp(&bsp, &rt(DevicePreference::Cpu), &mut |l| lines.push(l.to_owned())).expect("light");
    assert!(crate::validate::validate(&out).is_empty(), "{:?}", crate::validate::validate(&out));
    assert!(check_lighting(&out).is_empty(), "{:?}", check_lighting(&out));
    let classic = light_bsp(&bsp, &quick(), &mut |_| {}).expect("classic");
    let below = floor(&out, 64.0, 128.0);
    let below_classic = floor(&classic, 64.0, 128.0);
    assert!((below - below_classic).abs() < below_classic * 0.15, "under the light: {below} vs {below_classic}");
    let (near, far) = (floor(&out, 100.0, 40.0), floor(&out, 200.0, 40.0));
    assert!(near > far * 1.2, "near {near} far {far}");
    let (lit, shadow) = (floor(&out, 350.0, 10.0), floor(&out, 350.0, 128.0));
    assert!(shadow < lit * 0.5, "lit {lit} shadow {shadow}");
    assert!(shadow > 0.0, "bounced light reaches the shadow");
    assert!(lines.iter().any(|l| l.contains("ray tracing on CPU")), "{lines:?}");
}

#[test]
fn same_seed_same_lightmaps() {
    let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
    let a = light_bsp(&bsp, &rt(DevicePreference::Cpu), &mut |_| {}).expect("a");
    let b = light_bsp(&bsp, &rt(DevicePreference::Cpu), &mut |_| {}).expect("b");
    assert_eq!(a, b);
}

#[test]
fn no_light_leaks_through_a_wall() {
    let mut id = 100;
    let w = "DEV/DEV_MEASUREGENERIC01B";
    let wall = block(&mut id, DVec3::new(400.0, 0.0, 0.0), DVec3::new(416.0, 200.0, 256.0), w, w);
    let text = room(w, &light_entity(500, "330 40 64", "")).replacen(
        "}
entity",
        &format!(
            "{wall}}}
entity"
        ),
        1,
    );
    let bsp = compile_vmf(&text);
    let out = light_bsp(&bsp, &rt(DevicePreference::Cpu), &mut |_| {}).expect("light");
    let classic = light_bsp(&bsp, &quick(), &mut |_| {}).expect("classic");
    let lit = floor(&out, 396.0, 40.0);
    let behind = floor(&out, 420.0, 40.0);
    let behind_classic = floor(&classic, 420.0, 40.0);
    assert!(lit > 20.0, "{lit}");
    assert!(behind < lit * 0.05, "light leaked under the wall: {behind} next to {lit}");
    assert!(behind <= behind_classic * 1.5 + 1.0, "ray traced {behind}, classic {behind_classic}");
}

#[test]
fn gpu_and_cpu_agree() {
    let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
    let mut lines = Vec::new();
    let gpu = light_bsp(&bsp, &rt(DevicePreference::Gpu), &mut |l| lines.push(l.to_owned())).expect("gpu");
    assert!(!lines.iter().any(|l| l.contains("did not build")), "{lines:?}");
    if !lines.iter().any(|l| l.contains("ray tracing on GPU")) {
        eprintln!("no GPU here: {lines:?}");
        return;
    }
    let cpu = light_bsp(&bsp, &rt(DevicePreference::Cpu), &mut |_| {}).expect("cpu");
    let (r, _) = compare(&gpu, &cpu, false, 2.0);
    let (lo, mid, hi) = (pct(&r, 0.05), pct(&r, 0.5), pct(&r, 0.95));
    assert!((mid - 1.0).abs() < 0.03, "median GPU/CPU {mid}");
    assert!(lo > 0.85 && hi < 1.15, "GPU/CPU spread {lo} {hi}");
}

#[test]
#[ignore]
fn relight_map_with_ray_tracing() {
    let Ok(path) = std::env::var("RT_MAP") else { return };
    let valve = std::fs::read(&path).expect("map");
    let quality = std::env::var("RT_QUALITY").ok().and_then(|q| Quality::parse(&q)).unwrap_or(Quality::Normal);
    let device = std::env::var("RT_DEVICE").ok().and_then(|q| DevicePreference::parse(&q)).unwrap_or_default();
    let hdr = hp_bsp::Bsp::from_vec(valve.clone()).unwrap().lump_info(lump::LIGHTING_HDR).is_ok_and(|l| l.len > 0);
    let mut opts = RadOptions { hdr, ldr: true, ..RadOptions::default() };
    let classic = std::env::var("RT_CLASSIC").is_ok();
    opts.no_skybox_recurse = std::env::var("RT_NOSKY").is_ok();
    if let Some(b) = std::env::var("RT_BOUNCES").ok().and_then(|b| b.parse().ok()) {
        opts.bounces = b;
    }
    if !classic {
        let effects = if std::env::var("RT_NOFX").is_ok() { Effects::none() } else { Effects::default() };
        opts.raytracing = Some(RayTracing { device, effects, ..RayTracing::with_quality(quality) });
    }
    let t = std::time::Instant::now();
    let out = light_bsp(&valve, &opts, &mut |l| eprintln!("  {l}")).expect("light");
    eprintln!("{path}: {:.1} s", t.elapsed().as_secs_f64());
    if let Ok(save) = std::env::var("RT_SAVE") {
        std::fs::write(save, &out).unwrap();
    }
    assert!(check_lighting(&out).is_empty(), "{:?}", check_lighting(&out));
    for h in [false, true] {
        if h && !hdr {
            continue;
        }
        let (r, faces) = compare(&out, &valve, h, 2.0);
        eprintln!(
            "  {}: {faces} faces, {} luxels, ours/Valve p10 {:.3} p25 {:.3} median {:.3} p75 {:.3} p90 {:.3}",
            if h { "HDR" } else { "LDR" },
            r.len(),
            pct(&r, 0.1),
            pct(&r, 0.25),
            pct(&r, 0.5),
            pct(&r, 0.75),
            pct(&r, 0.9)
        );
    }
}

#[test]
#[ignore]
fn face_brightness_report() {
    let (Ok(a), Ok(b)) = (std::env::var("RT_OURS"), std::env::var("RT_VALVE")) else { return };
    let ours = std::fs::read(a).unwrap();
    let valve = std::fs::read(b).unwrap();
    let src = hp_bsp::Bsp::from_vec(valve.clone()).unwrap();
    let map = super::super::data::MapData::read(&src).unwrap();
    let names = super::super::texlights::texdata(&src);
    let oa = hp_bsp::Bsp::from_vec(ours).unwrap();
    let (fa, la) = (oa.lump(lump::FACES).unwrap().into_owned(), oa.lump(lump::LIGHTING).unwrap().into_owned());
    let (fb, lb) = (src.lump(lump::FACES).unwrap().into_owned(), src.lump(lump::LIGHTING).unwrap().into_owned());
    let i32_at = super::super::data::i32_at;
    let mut rows = Vec::new();
    for i in 0..fa.len().min(fb.len()) / 56 {
        let o = i * 56;
        let (pa, pb) = (i32_at(&fa, o + 20), i32_at(&fb, o + 20));
        if pa < 0 || pb < 0 {
            continue;
        }
        let n = ((i32_at(&fa, o + 36) + 1) * (i32_at(&fa, o + 40) + 1)) as usize;
        let (mut sa, mut sb) = (0.0f32, 0.0f32);
        for j in 0..n {
            let get = |l: &[u8], ofs: i32| {
                let at = ofs as usize + j * 4;
                l.get(at..at + 4).map(|c| super::super::output::decode([c[0], c[1], c[2], c[3]]))
            };
            if let (Some(x), Some(y)) = (get(&la, pa), get(&lb, pb)) {
                sa += x.x + x.y + x.z;
                sb += y.x + y.y + y.z;
            }
        }
        if sb < 3.0 * n as f32 {
            continue;
        }
        let face = &map.faces[i];
        let pts = map.face_points(face);
        let c = pts.iter().copied().sum::<DVec3>() / pts.len().max(1) as f64;
        let td = usize::try_from(face.texinfo).ok().and_then(|t| map.texinfo.get(t)).map_or(-1, |t| t.texdata);
        let name = usize::try_from(td).ok().and_then(|t| names.get(t)).map(|n| n.0.clone()).unwrap_or_default();
        let nrm = map.face_plane(face).map_or(DVec3::ZERO, |p| p.normal);
        rows.push((sa / sb, n, c, nrm, name, i, sb / n as f32));
    }
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    if std::env::var("RT_BRIGHTEST").is_ok() {
        rows.reverse();
    }
    for r in rows.iter().take(40) {
        eprintln!(
            "ratio {:.2} luxels {} at {:.0?} normal {:.2?} {} face {} valve avg {:.1}",
            r.0, r.1, r.2, r.3, r.4, r.5, r.6
        );
    }
    let dark = rows.iter().filter(|r| r.0 < 0.75).map(|r| r.1).sum::<usize>();
    let total = rows.iter().map(|r| r.1).sum::<usize>();
    eprintln!("{dark} of {total} luxels on faces darker than 0.75");
}

#[test]
#[ignore]
fn face_styles_report() {
    let Ok(face) = std::env::var("RT_FACE").map(|f| f.parse::<usize>().unwrap()) else { return };
    for path in std::env::var("RT_FILES").unwrap_or_default().split(';').filter(|p| !p.is_empty()) {
        let b = hp_bsp::Bsp::from_vec(std::fs::read(path).unwrap()).unwrap();
        let (fl, ll) = (b.lump(lump::FACES).unwrap().into_owned(), b.lump(lump::LIGHTING).unwrap().into_owned());
        let o = face * 56;
        let styles = &fl[o + 16..o + 20];
        let ofs = super::super::data::i32_at(&fl, o + 20);
        let n =
            ((super::super::data::i32_at(&fl, o + 36) + 1) * (super::super::data::i32_at(&fl, o + 40) + 1)) as usize;
        let mut avgs = Vec::new();
        let count = styles.iter().filter(|s| **s != 255).count();
        let per = n;
        for k in 0..count * 4 {
            let mut sum = 0.0f32;
            for j in 0..per {
                let at = ofs as usize + (k * per + j) * 4;
                if let Some(c) = ll.get(at..at + 4) {
                    let v = super::super::output::decode([c[0], c[1], c[2], c[3]]);
                    sum += v.x + v.y + v.z;
                }
            }
            avgs.push(sum / per as f32);
        }
        eprintln!("{path}: styles {styles:?} luxels {n} block averages {avgs:.1?}");
    }
}

#[test]
#[ignore]
fn face_info_report() {
    let Ok(path) = std::env::var("RT_MAP") else { return };
    let src = hp_bsp::Bsp::from_vec(std::fs::read(path).unwrap()).unwrap();
    let map = super::super::data::MapData::read(&src).unwrap();
    let names = super::super::texlights::texdata(&src);
    for f in std::env::var("RT_FACES").unwrap_or_default().split(',').filter_map(|v| v.parse::<usize>().ok()) {
        let face = &map.faces[f];
        let td = usize::try_from(face.texinfo).ok().and_then(|t| map.texinfo.get(t));
        let name = td.and_then(|t| usize::try_from(t.texdata).ok()).and_then(|t| names.get(t)).map(|n| n.0.clone());
        let pts = map.face_points(face);
        eprintln!("face {f}: {name:?} flags {:#x} disp {} pts {:.0?}", td.map_or(0, |t| t.flags), face.disp, pts);
    }
    for p in std::env::var("RT_POINTS").unwrap_or_default().split(';').filter(|p| !p.is_empty()) {
        let v: Vec<f64> = p.split_whitespace().filter_map(|x| x.parse().ok()).collect();
        let q = DVec3::new(v[0], v[1], v[2]);
        let head = map.models.first().map_or(-1, |m| m.head);
        let leaf = map.point_leaf(head, q).and_then(|l| map.leafs.get(l));
        eprintln!("point {q:?}: leaf contents {:?} cluster {:?}", leaf.map(|l| l.contents), leaf.map(|l| l.cluster));
    }
}

fn light_ent(pairs: &[(&str, &str)]) -> hp_bsp::Entity {
    hp_bsp::Entity { pairs: pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect() }
}

#[test]
fn engine_falloff_matches_vrad_conventions() {
    let map = super::super::data::MapData::default();
    let ents = [
        light_ent(&[("classname", "light"), ("origin", "0 0 0"), ("_light", "255 200 150 300")]),
        light_ent(&[
            ("classname", "light"),
            ("origin", "0 0 0"),
            ("_light", "255 255 255 200"),
            ("_constant_attn", "1"),
            ("_linear_attn", "0.02"),
            ("_quadratic_attn", "0.001"),
        ]),
        light_ent(&[
            ("classname", "light"),
            ("origin", "0 0 0"),
            ("_light", "255 255 255 255"),
            ("_fifty_percent_distance", "200"),
            ("_zero_percent_distance", "400"),
        ]),
        light_ent(&[
            ("classname", "light"),
            ("origin", "0 0 0"),
            ("_light", "255 255 255 255"),
            ("_fifty_percent_distance", "150"),
            ("_zero_percent_distance", "300"),
            ("_hardfalloff", "1"),
        ]),
        light_ent(&[
            ("classname", "light_spot"),
            ("origin", "0 0 0"),
            ("angles", "-90 0 0"),
            ("pitch", "-90"),
            ("_light", "255 255 255 400"),
            ("_cone", "40"),
            ("_inner_cone", "20"),
            ("_exponent", "2"),
        ]),
        light_ent(&[
            ("classname", "light_spot"),
            ("origin", "0 0 0"),
            ("angles", "-90 0 0"),
            ("pitch", "-90"),
            ("_light", "255 255 255 400"),
            ("_cone", "60"),
            ("_inner_cone", "60"),
            ("_exponent", "1"),
        ]),
    ];
    let lights = super::super::lights::parse(&ents, &[false], &map);
    assert_eq!(lights.list.len(), ents.len());
    let params = hp_raytrace::Params::of(Quality::Normal);
    for l in &lights.list {
        let e = super::bake::engine_light(l, lights.sun_extent, &params);
        assert_eq!(e.intensity[0], l.intensity[0].to_array());
        for k in 0..400 {
            let dist = 1.0 + k as f32 * 3.7;
            let a = k as f32 * 0.37;
            let dir = Vec3A::new(a.sin() * 0.8, a.cos() * 0.8, 0.6 - (k % 7) as f32 * 0.25).normalize();
            let (want, got) = (l.falloff(dir, dist), e.falloff(dir, dist));
            assert!((want - got).abs() <= want.abs() * 1e-6 + 1e-12, "{:?} at {dist}: {want} vs {got}", l.kind);
        }
    }
}

#[test]
fn spot_cone_lights_inside_and_not_outside() {
    let spot = "entity
{
	\"id\" \"600\"
	\"classname\" \"light_spot\"
	\"origin\" \"128 128 240\"
	\"angles\" \"-90 0 0\"
	\"pitch\" \"-90\"
	\"_light\" \"255 255 255 800\"
	\"_cone\" \"25\"
	\"_inner_cone\" \"15\"
	\"_exponent\" \"1\"
}
";
    let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", spot));
    let opts = RadOptions { bounces: 0, ..rt(DevicePreference::Cpu) };
    let out = light_bsp(&bsp, &opts, &mut |_| {}).expect("light");
    let classic = light_bsp(&bsp, &RadOptions { bounces: 0, ..quick() }, &mut |_| {}).expect("classic");
    let center = floor(&out, 128.0, 128.0);
    let edge = floor(&out, 128.0 + 240.0 * 20f64.to_radians().tan(), 128.0);
    let outside = floor(&out, 128.0 + 240.0 * 40f64.to_radians().tan(), 128.0);
    let center_classic = floor(&classic, 128.0, 128.0);
    assert!(center > 20.0, "{center}");
    assert!((center - center_classic).abs() < center_classic * 0.1, "{center} vs {center_classic}");
    assert!(edge < center && edge > 0.0, "{edge} {center}");
    assert!(outside < center * 0.01, "{outside}");
}

#[test]
fn progressive_refines_to_the_full_bake() {
    let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
    let opts = rt(DevicePreference::Cpu);
    let ctl = super::super::session::Control { cancelled: &|| false, progress: &|_, _| {}, partial: None };
    let mut p = super::Progressive::new(bsp, &opts, &crate::props::NoModels, &mut |_| {}, &ctl).expect("progressive");
    assert!(p.faces() > 0 && p.luxels() > 0);
    let first = p.refine(&ctl, 0.05).expect("refine");
    assert!(!first.faces.is_empty());
    let mut last = first;
    for _ in 0..200 {
        if last.finished {
            break;
        }
        last = p.refine(&ctl, 0.5).expect("refine");
    }
    assert!(last.finished, "{}", last.progress);
    let all = p.output_all();
    assert_eq!(all.len(), p.faces());
    let lit = all.iter().flat_map(|f| f.values.iter()).filter(|v| v[0] > 1.0).count();
    assert!(lit > 0);
    let _ = p.light_shafts();
}

pub(crate) fn shaft_test_map() -> String {
    let mut id = 1u32;
    let w = "DEV/DEV_MEASUREGENERIC01B";
    let sky = "TOOLS/TOOLSSKYBOX";
    let v = DVec3::new;
    let mut s = String::new();
    let mut b = |lo: DVec3, hi: DVec3, m: &str| s.push_str(&block(&mut id, lo, hi, m, m));
    b(v(-64.0, -64.0, -32.0), v(1088.0, 576.0, -16.0), sky);
    b(v(-64.0, -64.0, 640.0), v(1088.0, 576.0, 656.0), sky);
    b(v(-80.0, -64.0, -32.0), v(-64.0, 576.0, 656.0), sky);
    b(v(1088.0, -64.0, -32.0), v(1104.0, 576.0, 656.0), sky);
    b(v(-80.0, -80.0, -32.0), v(1104.0, -64.0, 656.0), sky);
    b(v(-80.0, 576.0, -32.0), v(1104.0, 592.0, 656.0), sky);
    b(v(-64.0, -64.0, -16.0), v(1088.0, 576.0, 0.0), w);
    for x0 in [48.0, 560.0] {
        let x1 = x0 + 416.0;
        b(v(x0, 48.0, 0.0), v(x0 + 16.0, 464.0, 256.0), w);
        if x0 < 100.0 {
            b(v(x1 - 16.0, 48.0, 0.0), v(x1, 464.0, 256.0), w);
        }
        b(v(x0 + 16.0, 48.0, 0.0), v(x1 - 16.0, 64.0, 256.0), w);
        b(v(x0 + 16.0, 448.0, 0.0), v(x1 - 16.0, 464.0, 256.0), w);
    }
    b(v(48.0, 48.0, 256.0), v(192.0, 464.0, 272.0), w);
    b(v(320.0, 48.0, 256.0), v(464.0, 464.0, 272.0), w);
    b(v(192.0, 48.0, 256.0), v(320.0, 192.0, 272.0), w);
    b(v(192.0, 320.0, 256.0), v(320.0, 464.0, 272.0), w);
    b(v(560.0, 48.0, 256.0), v(976.0, 464.0, 272.0), w);
    let mut id2 = 5000u32;
    let mut walls = String::new();
    let mut c = |lo: DVec3, hi: DVec3| walls.push_str(&block(&mut id2, lo, hi, w, w));
    c(v(960.0, 48.0, 0.0), v(976.0, 192.0, 256.0));
    c(v(960.0, 320.0, 0.0), v(976.0, 464.0, 256.0));
    c(v(960.0, 192.0, 0.0), v(976.0, 320.0, 64.0));
    c(v(960.0, 192.0, 176.0), v(976.0, 320.0, 256.0));
    let world = s + &walls;
    format!(
        "versioninfo\n{{\n\t\"editorversion\" \"400\"\n\t\"mapversion\" \"1\"\n}}\nworld\n{{\n\t\"id\" \"1\"\n\t\"mapversion\" \"1\"\n\t\"classname\" \"worldspawn\"\n\t\"skyname\" \"sky_day01_01\"\n{world}}}\n\
         entity\n{{\n\t\"id\" \"9000\"\n\t\"classname\" \"info_player_start\"\n\t\"origin\" \"256 256 16\"\n}}\n\
         entity\n{{\n\t\"id\" \"9001\"\n\t\"classname\" \"light_environment\"\n\t\"origin\" \"256 256 128\"\n\t\"_light\" \"255 240 220 400\"\n\t\"_ambient\" \"130 150 180 60\"\n\t\"pitch\" \"-55\"\n\t\"angles\" \"0 200 0\"\n\t\"SunSpreadAngle\" \"2\"\n}}\n\
         entity\n{{\n\t\"id\" \"9002\"\n\t\"classname\" \"light_spot\"\n\t\"origin\" \"700 256 240\"\n\t\"angles\" \"-90 0 0\"\n\t\"pitch\" \"-90\"\n\t\"_light\" \"255 230 200 900\"\n\t\"_cone\" \"30\"\n\t\"_inner_cone\" \"20\"\n}}\n"
    )
}

fn sprites_in(bsp: &[u8], lo: DVec3, hi: DVec3) -> usize {
    let src = hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap();
    src.entities()
        .unwrap()
        .iter()
        .filter(|e| {
            e.classname() == "prop_dynamic"
                && e.get("model").is_some_and(|m| m.starts_with("models/hammerpro/raytrace/vol_"))
        })
        .filter_map(|e| e.origin().map(DVec3::from_array))
        .filter(|p| p.cmpge(lo).all() && p.cmple(hi).all())
        .count()
}

#[test]
fn sunlight_and_spots_make_light_shafts_in_rooms() {
    let text = shaft_test_map();
    if let Ok(dir) = std::env::var("RT_WRITE_SHAFT_MAP") {
        std::fs::write(dir, &text).unwrap();
    }
    let bsp = compile_vmf(&text);
    let opts = RadOptions {
        raytracing: Some(RayTracing {
            device: DevicePreference::Cpu,
            min_samples: Some(8),
            max_samples: Some(16),
            effects: Effects { shafts: true, ..Effects::default() },
            ..RayTracing::with_quality(Quality::Draft)
        }),
        ..quick()
    };
    let mut lines = Vec::new();
    let out = light_bsp(&bsp, &opts, &mut |l| lines.push(l.to_owned())).expect("light");
    let skylight = sprites_in(&out, DVec3::new(64.0, 64.0, 0.0), DVec3::new(448.0, 448.0, 400.0));
    let window = sprites_in(&out, DVec3::new(576.0, 64.0, 0.0), DVec3::new(992.0, 448.0, 400.0));
    let log: Vec<&String> = lines.iter().filter(|l| l.contains("effects")).collect();
    assert!(skylight > 0, "no shaft under the skylight: {log:?}");
    assert!(window > 0, "no shaft in the window room: {log:?}");
    let suns: usize = log
        .iter()
        .find_map(|l| {
            l.split(" volumetric light beams (").nth(1).and_then(|r| r.split(' ').next()).and_then(|n| n.parse().ok())
        })
        .unwrap_or(0);
    assert!(suns >= 2, "the skylight and the window each make a sun shaft: {log:?}");
}

#[test]
#[ignore]
fn preview_pacing_measure() {
    let Ok(path) = std::env::var("RT_MAP") else { return };
    let bsp = std::fs::read(&path).expect("map");
    for (submit, share) in [(0.08f64, 1.0f32), (0.004, 1.0), (0.004, 0.6), (0.004, 0.4)] {
        let opts = RadOptions {
            hdr: true,
            ldr: false,
            raytracing: Some(RayTracing {
                max_samples: Some(32),
                min_samples: Some(16),
                ..RayTracing::with_quality(Quality::Draft)
            }),
            ..RadOptions::default()
        };
        let ctl = super::super::session::Control { cancelled: &|| false, progress: &|_, _| {}, partial: None };
        let mut p = super::Progressive::new(bsp.clone(), &opts, &crate::props::NoModels, &mut |_| {}, &ctl).expect("p");
        p.set_pacing_for_tests(submit, share);
        let t = std::time::Instant::now();
        let mut longest = 0.0f64;
        let mut calls = 0;
        loop {
            let c = std::time::Instant::now();
            let r = p.refine(&ctl, 0.1).expect("refine");
            longest = longest.max(c.elapsed().as_secs_f64());
            calls += 1;
            if calls % 20 == 0 {
                eprintln!(
                    "  {calls} calls, {:.1} s, progress {:.3}, spl {:.1}",
                    t.elapsed().as_secs_f64(),
                    r.progress,
                    r.samples_per_luxel
                );
            }
            if r.finished || calls > 3000 {
                break;
            }
        }
        eprintln!(
            "submit {submit} share {share}: converged in {:.1} s, {calls} refine calls, longest {:.3} s",
            t.elapsed().as_secs_f64(),
            longest
        );
    }
}

fn face_sizes(bsp: &[u8]) -> Vec<(i16, i32, i32)> {
    let b = hp_bsp::Bsp::from_vec(bsp.to_vec()).expect("bsp");
    let faces = b.lump(crate::emit::lump::FACES).expect("faces").into_owned();
    faces
        .chunks_exact(56)
        .map(|r| {
            let at = |o: usize| i32::from_le_bytes([r[o], r[o + 1], r[o + 2], r[o + 3]]);
            (i16::from_le_bytes([r[10], r[11]]), at(36), at(40))
        })
        .collect()
}

#[test]
fn final_refines_lightmaps_within_the_limits_and_stays_correct() {
    let bsp = compile_vmf(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 128", "")));
    let opts = |detail: Option<f32>| RadOptions {
        raytracing: Some(RayTracing {
            device: DevicePreference::Cpu,
            min_samples: Some(32),
            max_samples: Some(64),
            luxel_detail: detail,
            effects: Effects::none(),
            ..RayTracing::with_quality(Quality::Draft)
        }),
        ..quick()
    };
    let mut lines = Vec::new();
    let fine = light_bsp(&bsp, &opts(Some(2.0)), &mut |l| lines.push(l.to_owned())).expect("fine");
    let coarse = light_bsp(&bsp, &opts(None), &mut |_| {}).expect("coarse");
    assert!(crate::validate::validate(&fine).is_empty(), "{:?}", crate::validate::validate(&fine));
    assert!(check_lighting(&fine).is_empty(), "{:?}", check_lighting(&fine));
    assert!(lines.iter().any(|l| l.contains("finer lightmaps, luxel detail x2.00")), "{lines:?}");
    let (a, b) = (face_sizes(&coarse), face_sizes(&fine));
    assert_eq!(a.len(), b.len());
    let limit = hp_bsp::limits::LIGHTMAP_EXTENT.max_in(quick().limits) as i32;
    let mut grown = 0;
    for ((_, w0, h0), (_, w1, h1)) in a.iter().zip(&b) {
        assert!(*w1 < limit && *h1 < limit, "{w1}x{h1} over {limit}");
        if (w1 + 1) * (h1 + 1) > (w0 + 1) * (h0 + 1) * 3 {
            grown += 1;
        }
    }
    assert!(grown > 0, "no face got finer lightmaps");
    let (x, y) = (floor(&fine, 64.0, 128.0), floor(&coarse, 64.0, 128.0));
    assert!((x - y).abs() < y * 0.15, "same exposure under the light: {x} vs {y}");
}

#[test]
fn final_area_lights_are_soft_and_fit_their_room() {
    let n = hp_raytrace::Params::of(Quality::Normal);
    let f = hp_raytrace::Params::of(Quality::Final);
    assert!(f.light_radius > n.light_radius * 2.0 && f.sun_soft_angle > 0.0 && f.luxel_detail > 1.0);
    assert_eq!((n.luxel_detail, n.sun_soft_angle), (1.0, 0.0));
    let tris = [hp_light::bvh::Triangle {
        p: [Vec3A::new(-512.0, -512.0, 4.0), Vec3A::new(512.0, -512.0, 4.0), Vec3A::new(0.0, 512.0, 4.0)],
        payload: 0,
    }];
    let bvh = hp_light::bvh::Bvh::new(&tris);
    let mut light = hp_raytrace::Light { kind: hp_raytrace::LightKind::Point, size: 8.0, ..Default::default() };
    super::bake::fit_light_size(&mut light, &bvh, 1.0, false);
    assert!(light.size <= 3.01 && light.size >= 2.0, "a light 4 units under a ceiling: {}", light.size);
    let mut open = hp_raytrace::Light {
        kind: hp_raytrace::LightKind::Point,
        size: 8.0,
        position: [0.0, 0.0, -200.0],
        ..Default::default()
    };
    super::bake::fit_light_size(&mut open, &bvh, 1.0, false);
    assert_eq!(open.size, 8.0);
}

#[test]
fn final_contact_shadows_reach_the_direct_light() {
    let cine = |q: Quality| hp_raytrace::Params::of(q).cinematic;
    assert!(!cine(Quality::Normal) && !cine(Quality::Draft) && cine(Quality::Final) && cine(Quality::Optimal));
    assert_eq!(super::contact_strengths(false, 0.5), (0.0, 0.5));
    let (d, i) = super::contact_strengths(true, 0.5);
    assert!(d > 0.3 && d <= 0.6 && i == 1.0, "{d} {i}");
    assert_eq!(super::contact_strengths(true, 0.0), (0.0, 0.0));
    assert_eq!(super::contact_strengths(true, 1.0).1, 1.0);
    let classic = RayTracing { look: Some(super::Look::Classic), ..RayTracing::with_quality(Quality::Final) };
    assert!(!classic.params().cinematic && !classic.params().contact_ao);
}

#[test]
fn cinematic_lights_make_pools_and_fixtures_shine_down() {
    let fifty = hp_raytrace::Light {
        kind: hp_raytrace::LightKind::Point,
        intensity: [[100.0; 3]; 2],
        constant: 1.0,
        linear: 0.0,
        quadratic: 1.0 / (200.0 * 200.0),
        cap: 1.0e22,
        fade: (0.0, -1.0),
        reach: 800.0,
        size: 8.0,
        ..Default::default()
    };
    let none = hp_light::bvh::Bvh::new(&[]);
    let mut pool = fifty;
    assert!(!super::bake::cinematic_light(&mut pool, &none), "no ceiling above");
    let dir = Vec3A::NEG_Z;
    let at = |l: &hp_raytrace::Light, d: f32| l.falloff(dir, d) * l.intensity[0][0];
    let dref = super::bake::reference_distance(&fifty);
    assert!((dref - 200.0).abs() < 2.0, "{dref}");
    assert!(at(&pool, 40.0) > at(&fifty, 40.0) * 1.5, "bright pool near the lamp");
    assert!(at(&pool, 40.0) < at(&fifty, 40.0) * 4.5, "no blown highlight");
    assert!((at(&pool, 200.0) / at(&fifty, 200.0) - 1.3).abs() < 0.05, "same brightness at the reference distance");
    assert!(at(&pool, 600.0) <= at(&fifty, 600.0) * 1.01, "darker far away");
    assert_eq!(at(&pool, 801.0), 0.0);
    let ceiling = [hp_light::bvh::Triangle {
        p: [Vec3A::new(-512.0, -512.0, 6.0), Vec3A::new(512.0, -512.0, 6.0), Vec3A::new(0.0, 512.0, 6.0)],
        payload: 0,
    }];
    let bvh = hp_light::bvh::Bvh::new(&ceiling);
    let mut fixture = fifty;
    assert!(super::bake::cinematic_light(&mut fixture, &bvh));
    assert!(fixture.falloff(Vec3A::Z, 100.0) > 0.0 && fixture.falloff(Vec3A::NEG_Z, 100.0) == 0.0);
    let mut flat = hp_raytrace::Light { quadratic: 0.0, ..fifty };
    assert!(!super::bake::cinematic_light(&mut flat, &bvh));
    assert_eq!(flat.constant, 1.0, "lights without falloff are kept");
    let mut kneed = vec![pool];
    super::bake::knee_lights(&mut kneed, std::slice::from_ref(&fifty));
    for k in 0..60 {
        let d = 1.0 + k as f32 * 4.0;
        assert!(
            at(&kneed[0], d) <= at(&fifty, d).max(at(&fifty, 200.0) * 0.25) * super::bake::KNEE * 1.02,
            "hot spot at {d}"
        );
    }
    let warm = super::bake::richer_bounce([0.6, 0.4, 0.2]);
    let lum = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    assert!((lum(warm) - lum([0.6, 0.4, 0.2])).abs() < 1e-4 && warm[0] > 0.6 && warm[2] < 0.2);
}

fn named_sun_map() -> String {
    shaft_test_map().replace(
        "\"classname\" \"light_environment\"\n",
        "\"classname\" \"light_environment\"\n\t\"targetname\" \"sun_day\"\n",
    )
}

fn all_styles_at(bsp: &[u8], p: DVec3) -> f32 {
    (0u8..=64).filter_map(|s| super::super::tests::style_luxel_at(bsp, p, DVec3::Z, s)).map(|v| v.x).sum()
}

fn crowded_styles_map() -> String {
    let lamps: String = [("a", "256 100 128"), ("b", "256 400 128"), ("c", "300 256 40"), ("d", "200 256 40")]
        .iter()
        .enumerate()
        .map(|(i, (name, at))| light_entity(9100 + i as u32, at, &format!("\t\"targetname\" \"lamp_{name}\"\n")))
        .collect();
    format!("{}{lamps}", named_sun_map())
}

#[test]
fn named_sun_keeps_its_style_slot_when_more_styles_reach_a_face() {
    let alone = compile_vmf(&named_sun_map());
    let crowded = compile_vmf(&crowded_styles_map());
    let open = DVec3::new(1040.0, 520.0, 0.0);
    for (name, opts) in [("classic", quick()), ("ray tracing", rt(DevicePreference::Cpu))] {
        let a = all_styles_at(&light_bsp(&alone, &opts, &mut |_| {}).expect("alone"), open);
        let c = all_styles_at(&light_bsp(&crowded, &opts, &mut |_| {}).expect("crowded"), open);
        assert!(a > 0.0, "{name}: the sun lights the open floor");
        assert!(c > a * 0.8, "{name}: the sun lost its slot to lamps behind walls: {c} vs {a}");
    }
}

#[test]
fn strongest_slots_keep_style_zero_and_the_brightest_styles() {
    let keep = super::super::bake::strongest_slots(&[0.0, 0.0, 0.1, 0.0, 9.0, 3.0, 0.2], 4);
    assert_eq!(keep, vec![0, 4, 5, 6]);
    assert_eq!(super::super::bake::strongest_slots(&[1.0, 0.0, 0.0], 4), vec![0]);
}

#[test]
fn cinematic_strong_lights_keep_their_far_light() {
    let hall = hp_raytrace::Light {
        kind: hp_raytrace::LightKind::Point,
        intensity: [[2000.0 * 10000.0 / 255.0; 3]; 2],
        constant: 0.0,
        linear: 0.0,
        quadratic: 1.0,
        cap: 1.0e22,
        fade: (0.0, -1.0),
        reach: 6000.0,
        size: 8.0,
        ..Default::default()
    };
    let none = hp_light::bvh::Bvh::new(&[]);
    let mut pool = hall;
    super::bake::cinematic_light(&mut pool, &none);
    let mut lights = vec![pool];
    super::bake::knee_lights(&mut lights, std::slice::from_ref(&hall));
    let dir = Vec3A::NEG_Z;
    let at = |l: &hp_raytrace::Light, d: f32| l.falloff(dir, d) * l.intensity[0][0];
    for d in [300.0, 600.0, 900.0, 1400.0, 2500.0] {
        assert!(at(&lights[0], d) >= at(&hall, d) * 0.9, "darker than the authored light at {d}");
    }
}

#[test]
fn volumetric_beams_are_off_unless_asked_for() {
    assert!(!Effects::default().shafts);
    let bsp = compile_vmf(&shaft_test_map());
    let opts = RadOptions {
        raytracing: Some(RayTracing {
            device: DevicePreference::Cpu,
            min_samples: Some(8),
            max_samples: Some(16),
            ..RayTracing::with_quality(Quality::Draft)
        }),
        ..quick()
    };
    let out = light_bsp(&bsp, &opts, &mut |_| {}).expect("light");
    assert_eq!(sprites_in(&out, DVec3::splat(-1.0e5), DVec3::splat(1.0e5)), 0);
}
