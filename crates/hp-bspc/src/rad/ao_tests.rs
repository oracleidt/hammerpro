use glam::DVec3;

use super::tests::{GrassMats, Mats, block, light_entity, luxel_at, room, room_with};
use super::*;
use crate::materials::MaterialSource;

fn opts(ao: Option<bool>, threads: usize) -> RadOptions {
    RadOptions {
        hdr: true,
        ldr: true,
        bounces: 8,
        quality: RadQuality::Normal,
        threads: Some(threads),
        ao: AoOptions { enabled: ao, ..AoOptions::default() },
        ..RadOptions::default()
    }
}

fn compile_with(text: &str, mats: &dyn MaterialSource) -> Vec<u8> {
    let map = hp_vmf::parse(text).expect("vmf");
    let out = crate::compile(&map, mats, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |_| {})
        .expect("compile");
    assert!(out.leak.is_none(), "the test map leaks");
    out.bsp
}

fn floor(bsp: &[u8], x: f64, y: f64) -> f32 {
    let v = luxel_at(bsp, DVec3::new(x, y, 0.0), DVec3::Z, false).expect("floor luxel");
    v.x + v.y + v.z
}

#[test]
fn occlusion_darkens_corners_only() {
    let bsp = compile_with(&room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 200", "")), &Mats);
    let mut lines = Vec::new();
    let on = light_bsp(&bsp, &opts(Some(true), 2), &mut |l| lines.push(l.to_owned())).expect("light");
    let off = light_bsp(&bsp, &opts(Some(false), 2), &mut |_| {}).expect("light");
    assert!(crate::validate::validate(&on).is_empty());
    assert!(lines.iter().any(|l| l.starts_with("ambient occlusion: ") && l.contains("rays per luxel")), "{lines:?}");
    let ratio = |x: f64, y: f64| floor(&on, x, y) / floor(&off, x, y);
    let (wall, pillar) = (ratio(504.0, 128.0), ratio(280.0, 128.0));
    eprintln!(
        "AO on / off: wall foot {wall:.3}, pillar foot {pillar:.3}, corner {:.3}, open {:.3}, under the light {:.3}",
        ratio(504.0, 8.0),
        ratio(400.0, 128.0),
        ratio(64.0, 128.0)
    );
    assert!(wall < 0.97 && wall > 0.5, "wall foot {wall}");
    assert!(pillar < 0.97 && pillar > 0.5, "pillar foot {pillar}");
    let open = ratio(400.0, 128.0);
    assert!(open > 0.99 && open <= 1.0001, "open floor {open}");
    assert!(ratio(504.0, 8.0) < wall, "corner {} wall {wall}", ratio(504.0, 8.0));
    assert!(ratio(64.0, 128.0) > 0.97, "{}", ratio(64.0, 128.0));
    let along: Vec<f32> = (1..15).map(|k| ratio(504.0, 16.0 * k as f64 + 8.0)).collect();
    for w in along.windows(2) {
        assert!((w[0] - w[1]).abs() < 0.05, "{along:?}");
    }
}

#[test]
fn worldspawn_keys() {
    let keyed = |pairs: &str| {
        let text = room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 200", ""));
        let text =
            text.replacen("\"classname\" \"worldspawn\"\n", &format!("\"classname\" \"worldspawn\"\n{pairs}"), 1);
        compile_with(&text, &Mats)
    };
    let plain = keyed("");
    let base = light_bsp(&plain, &opts(None, 2), &mut |_| {}).expect("light");
    let on = light_bsp(&keyed("\t\"_ao\" \"1\"\n"), &opts(None, 2), &mut |_| {}).expect("light");
    let half =
        light_bsp(&keyed("\t\"_ao\" \"1\"\n\t\"_aostrength\" \"0.3\"\n"), &opts(None, 2), &mut |_| {}).expect("light");
    let (b, o, h) = (floor(&base, 504.0, 128.0), floor(&on, 504.0, 128.0), floor(&half, 504.0, 128.0));
    assert!(o < b * 0.97 && h < b && h > o, "off {b} strong {o} weak {h}");
    let mut by_default = opts(None, 2);
    by_default.ao.by_default = true;
    let def = light_bsp(&plain, &by_default, &mut |_| {}).expect("light");
    assert!(floor(&def, 504.0, 128.0) < b * 0.97);
    let off = light_bsp(&keyed("\t\"_ao\" \"0\"\n"), &by_default, &mut |_| {}).expect("light");
    assert_eq!(floor(&off, 504.0, 128.0), b);
    let mut lines = Vec::new();
    let none = RadOptions { bounces: 0, ..opts(Some(true), 2) };
    light_bsp(&plain, &none, &mut |l| lines.push(l.to_owned())).expect("light");
    assert!(lines.iter().any(|l| l.contains("ambient occlusion: off")), "{lines:?}");
}

fn busy_map() -> Vec<u8> {
    let sun = "entity\n{\n\t\"id\" \"600\"\n\t\"classname\" \"light_environment\"\n\t\"origin\" \"64 64 64\"\n\t\"_light\" \"255 240 220 300\"\n\t\"_ambient\" \"120 140 170 40\"\n\t\"pitch\" \"-50\"\n\t\"angles\" \"0 30 0\"\n\t\"SunSpreadAngle\" \"5\"\n}\n";
    let spot = "entity\n{\n\t\"id\" \"601\"\n\t\"classname\" \"light_spot\"\n\t\"origin\" \"400 200 200\"\n\t\"angles\" \"-90 0 0\"\n\t\"pitch\" \"-90\"\n\t\"_light\" \"255 200 150 400\"\n\t\"_inner_cone\" \"30\"\n\t\"_cone\" \"45\"\n}\n";
    let ents = format!(
        "{}{}{sun}{spot}",
        light_entity(500, "64 128 128", ""),
        light_entity(501, "448 64 128", "\t\"targetname\" \"switch\"\n")
    );
    let base = room_with("NATURE/GRASS", "TOOLS/TOOLSSKYBOX", &ents);
    let mut id = 700;
    let shelf = block(
        &mut id,
        DVec3::new(16.0, 160.0, 100.0),
        DVec3::new(112.0, 240.0, 108.0),
        "DEV/DEV_MEASUREGENERIC01B",
        "DEV/DEV_MEASUREGENERIC01B",
    );
    let at = base.find("\tsolid\n").expect("a solid");
    let text = format!("{}{shelf}{}", &base[..at], &base[at..]);
    let mut map = hp_vmf::parse(&text).expect("vmf");
    let solid = map
        .world
        .solids
        .iter_mut()
        .find(|s| s.sides.iter().any(|d| d.plane.iter().all(|p| p.z == 108.0)))
        .expect("shelf");
    let top = solid.sides.iter_mut().find(|d| d.plane.iter().all(|p| p.z == 108.0)).expect("top");
    let mut disp = hp_vmf::DispInfo::new_flat(3, DVec3::new(16.0, 160.0, 108.0), DVec3::Z);
    for (k, d) in disp.distances.iter_mut().enumerate() {
        *d = ((k * 7) % 5) as f64 * 2.0;
    }
    top.disp = Some(Box::new(disp));
    let out = crate::compile(&map, &GrassMats, &crate::props::NoModels, &crate::CompileOptions::default(), &mut |_| {})
        .expect("compile");
    assert!(out.leak.is_none());
    out.bsp
}

fn lighting_lumps(bsp: &[u8]) -> Vec<(usize, Vec<u8>)> {
    let b = hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap();
    [
        lump::LIGHTING,
        lump::LIGHTING_HDR,
        lump::FACES,
        lump::FACES_HDR,
        lump::LEAF_AMBIENT_LIGHTING,
        lump::LEAF_AMBIENT_LIGHTING_HDR,
        15,
        54,
        lump::LEAFS,
    ]
    .iter()
    .map(|&l| (l, b.lump(l).map(|c| c.into_owned()).unwrap_or_default()))
    .collect()
}

#[test]
fn same_bytes_on_any_thread_count() {
    let bsp = busy_map();
    let first = light_bsp(&bsp, &opts(Some(true), 1), &mut |_| {}).expect("light");
    let lumps = lighting_lumps(&first);
    assert!(lumps.iter().find(|l| l.0 == lump::LIGHTING).is_some_and(|l| !l.1.is_empty()));
    let g = hp_bsp::Bsp::from_vec(first.clone()).unwrap().game_lumps().unwrap();
    assert!(g.get(crate::detail_props::DPRP).is_some(), "the floor has detail props");
    for threads in [1, 2, 3, 8, 8] {
        let again = light_bsp(&bsp, &opts(Some(true), threads), &mut |_| {}).expect("light");
        for ((l, a), (_, b)) in lumps.iter().zip(lighting_lumps(&again)) {
            assert!(*a == b, "lump {l} differs on {threads} threads");
        }
        assert!(first == again, "the file differs on {threads} threads");
    }
    let fin = |threads| RadOptions { quality: RadQuality::Final, ..opts(Some(true), threads) };
    let a = light_bsp(&bsp, &fin(2), &mut |_| {}).expect("light");
    let b = light_bsp(&bsp, &fin(7), &mut |_| {}).expect("light");
    assert!(a == b, "final quality differs between 2 and 7 threads");
}

fn with_ctx(bsp: &[u8], quality: RadQuality, f: impl FnOnce(&bake::Ctx<'_>)) {
    let src = hp_bsp::Bsp::from_vec(bsp.to_vec()).unwrap();
    let map = data::MapData::read(&src).unwrap();
    let passes = [false, true];
    let lights = lights::parse(&map.entities, &passes, &map);
    let params = bake::Params::of(quality);
    let all = surface::lit_faces(&map, 45.0);
    let faces: Vec<surface::LitFace> = all.iter().flatten().cloned().collect();
    let lit_index = bake::lit_index(map.faces.len(), &faces);
    let (shadow, _) = surface::shadow_scene(&map, &all, Vec::new(), true);
    let (surf, surf_info) = surface::surface_scene(&map, &all, &lit_index);
    let blockers = surface::blocker_scene(&map);
    let occ = props::Occluders::default();
    let (cameras, camera_areas) = bake::sky_cameras(&map);
    let ctx = bake::Ctx {
        map: &map,
        faces: &faces,
        shadow: &shadow,
        surf: &surf,
        surf_info: &surf_info,
        lights: &lights,
        passes: 2,
        params,
        light_rows: bake::Ctx::rows(&map, &lights, &surface::sky_clusters(&map)),
        sky_dirs: bake::sphere_dirs(params.sky_dirs),
        sun_dirs: bake::sphere_dirs(params.sun_samples.max(1)),
        cameras,
        camera_areas,
        props: &occ,
        ao: None,
        blockers: &blockers,
        work: &super::progress::Work::new(),
    };
    f(&ctx);
}

#[test]
fn sky_light_apart_adds_up() {
    let bsp = busy_map();
    with_ctx(&bsp, RadQuality::Normal, |ctx| {
        let cand: Vec<(usize, usize)> = (0..ctx.lights.list.len()).map(|i| (i, 0)).collect();
        let lights = sky::lights_of(ctx, &cand);
        assert_eq!(lights.len(), 1, "the sky ambient of the light_environment");
        let mut checked = 0;
        for f in ctx.faces {
            let nn = if f.bump { 4 } else { 1 };
            let stride = 2 * nn;
            let samples: Vec<Option<bake::Sample>> =
                (0..f.luxels()).map(|i| Some(ctx.sample(f, (i % f.w) as f64, (i / f.w) as f64))).collect();
            let mut acc = vec![glam::Vec3A::ZERO; f.luxels() * stride];
            for (i, sp) in samples.iter().enumerate() {
                ctx.gather(
                    &cand,
                    1,
                    nn,
                    sp.as_ref().unwrap(),
                    bake::Part::Direct,
                    &mut acc[i * stride..(i + 1) * stride],
                );
            }
            let sky = sky::face(ctx, &lights, (1, nn, samples.len()), &samples, &mut acc);
            assert_eq!(sky.len(), acc.len());
            for (i, sp) in samples.iter().enumerate() {
                let mut all = vec![glam::Vec3A::ZERO; stride];
                ctx.gather(&cand, 1, nn, sp.as_ref().unwrap(), bake::Part::All, &mut all);
                for k in 0..stride {
                    let (a, b) = (acc[i * stride + k], all[k]);
                    assert!((a - b).abs().max_element() <= b.abs().max_element() * 1e-5 + 1e-4, "{a} {b}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 1000);
    });
}

#[test]
fn blockers_stop_rays_like_the_shadow_scene() {
    let mut id = 800;
    let slab = block(
        &mut id,
        DVec3::new(300.0, 64.0, 120.0),
        DVec3::new(460.0, 192.0, 128.0),
        "TOOLS/TOOLSNODRAW",
        "TOOLS/TOOLSNODRAW",
    );
    let base = room("DEV/DEV_MEASUREGENERIC01B", &light_entity(500, "64 128 200", ""));
    let at = base.find("\tsolid\n").expect("a solid");
    let text = format!("{}{slab}{}", &base[..at], &base[at..]);
    let bsp = compile_with(&text, &Mats);
    with_ctx(&bsp, RadQuality::Normal, |ctx| {
        let dirs = bake::sphere_dirs(162);
        let (mut same, mut all) = (0, 0);
        for p in [
            DVec3::new(380.0, 128.0, 180.0),
            DVec3::new(380.0, 128.0, 60.0),
            DVec3::new(100.0, 40.0, 100.0),
            DVec3::new(480.0, 230.0, 20.0),
        ] {
            let p = p.as_vec3a();
            for d in &dirs {
                let old = ctx.trace_surface_if(p, *d, &|t| {
                    t > 1.0 && ctx.shadow.occluded_in(props::CLASS_WORLD, p, *d, t - 0.5, |_| true)
                });
                let new = ctx.trace_surface(p, *d);
                all += 1;
                same += usize::from(old == new);
            }
        }
        assert_eq!(same, all, "{same} of {all} rays alike");
        let above = DVec3::new(380.0, 128.0, 180.0).as_vec3a();
        assert_eq!(ctx.trace_surface(above, -glam::Vec3A::Z), bake::SurfHit::None);
    });
}

#[test]
fn detail_props_take_every_style() {
    let lights = light_entity(500, "64 128 128", "") + &light_entity(501, "448 128 128", "\t\"style\" \"200\"\n");
    let bsp = compile_with(&room("NATURE/GRASS", &lights), &GrassMats);
    let lit = light_bsp(&bsp, &opts(Some(false), 2), &mut |_| {}).expect("light");
    let b = hp_bsp::Bsp::from_vec(lit).unwrap();
    let g = b.game_lumps().unwrap();
    let records =
        crate::detail_props::parse_styles(&g.get(crate::detail_props::DPLT).expect("dplt").data().unwrap()).unwrap();
    assert!(
        records.iter().any(|(c, s)| *s == 200 && output::decode(*c).max_element() > 0.0),
        "{:?}",
        &records[..records.len().min(8)]
    );
}
