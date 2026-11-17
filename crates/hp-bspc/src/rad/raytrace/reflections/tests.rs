use glam::{DVec3, Vec3A};

use super::render::{cube_lookup, face_dir, sky_lookup};
use super::vtf;
use crate::materials::{MaterialFacts, MaterialSource, facts_by_name};
use crate::rad::raytrace::{DevicePreference, Effects, Quality, RayTracing};
use crate::rad::{RadOptions, RadQuality};

const ENGINE_FACES: [([f32; 3], [f32; 3]); 6] = [
    ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ([-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ([0.0, 1.0, 0.0], [0.0, 0.0, -1.0]),
    ([0.0, -1.0, 0.0], [0.0, 0.0, 1.0]),
    ([0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
    ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
];

const SKY_COLORS: [[u8; 3]; 6] = [[255, 0, 0], [0, 255, 0], [0, 0, 255], [255, 255, 0], [0, 255, 255], [255, 0, 255]];

#[test]
fn cubemap_faces_follow_the_engine_cube_layout() {
    for (face, (forward, up)) in ENGINE_FACES.iter().enumerate() {
        let (f, u) = (Vec3A::from_array(*forward), Vec3A::from_array(*up));
        let right = u.cross(f);
        assert!((face_dir(face, 0.0, 0.0) - f).length() < 1e-6, "face {face} looks along {f}");
        assert!((face_dir(face, 1.0, 0.0) - (f + right)).length() < 1e-6, "face {face}: +s goes right");
        assert!((face_dir(face, 0.0, -1.0) - (f + u)).length() < 1e-6, "face {face}: the first row is up");
        for (s, t) in [(-0.75f32, -0.5f32), (0.3, 0.9), (0.95, -0.95), (0.0, 0.0)] {
            let (back, u, v) = cube_lookup(face_dir(face, s, t));
            assert_eq!(back, face);
            assert!((u * 2.0 - 1.0 - s).abs() < 1e-5 && (v * 2.0 - 1.0 - t).abs() < 1e-5, "{face} {s} {t}: {u} {v}");
        }
    }
}

#[test]
fn sky_faces_and_cubemap_faces_share_names() {
    for face in 0..6 {
        let (sky, u, v) = sky_lookup(face_dir(face, 0.0, 0.0));
        assert_eq!(sky, face, "VTF face {face} is skybox face {face} (rt lf bk ft up dn)");
        assert!((u - 0.5).abs() < 1e-6 && (v - 0.5).abs() < 1e-6);
    }
    let (_, _, v) = sky_lookup(Vec3A::new(1.0, 0.0, 0.5));
    assert!(v < 0.5, "the top of a side face of the sky is up");
}

#[test]
fn half_floats_round_trip() {
    for v in [0.0f32, 1.0, 0.5, 0.333, 2.75, 100.0, 60_000.0, 1e-5, 3e-7] {
        let back = vtf::f16_to_f32(vtf::f32_to_f16(v));
        assert!((back - v).abs() <= v * 0.001 + 1e-7, "{v} -> {back}");
    }
    assert_eq!(vtf::f32_to_f16(f32::NAN), 0);
    assert_eq!(vtf::f16_to_f32(vtf::f32_to_f16(1e9)), 65_504.0);
}

fn colored_faces(size: u32) -> [Vec<Vec3A>; 6] {
    std::array::from_fn(|f| {
        let c = Vec3A::new(SKY_COLORS[f][0] as f32, SKY_COLORS[f][1] as f32, SKY_COLORS[f][2] as f32) / 255.0 * 0.8;
        vec![c; (size * size) as usize]
    })
}

#[test]
fn written_cubemaps_have_the_buildcubemaps_layout() {
    for hdr in [false, true] {
        let size = 16;
        let data = vtf::cubemap(&colored_faces(size), size, hdr);
        let info = hp_vtf::read_info(&data).expect("vtf");
        assert!(info.is_cubemap());
        assert_eq!((info.width, info.height), (size, size));
        assert_eq!(info.mip_count, 5);
        assert_eq!(info.flags, if hdr { vtf::FLAGS_HDR } else { vtf::FLAGS_LDR });
        assert_eq!(info.format.raw(), if hdr { vtf::FORMAT_RGBA16161616F } else { vtf::FORMAT_DXT1 });
        assert_eq!(info.faces, 7);
        for face in 0..6u32 {
            let level = hp_vtf::raw_level(&data, 0, 0, face).expect("level");
            let want = SKY_COLORS[face as usize].map(|c| f32::from(c) / 255.0 * 0.8);
            #[allow(clippy::needless_range_loop)]
            for k in 0..3 {
                let got = if hdr {
                    vtf::f16_to_f32(u16::from_le_bytes([level.data[2 * k], level.data[2 * k + 1]]))
                } else {
                    let img = level.to_rgba8().expect("rgba");
                    f32::from(img.pixels[k]) / 255.0
                };
                let w = if hdr { want[k] } else { f32::from(vtf::to_ldr(Vec3A::splat(want[k]))[0]) / 255.0 };
                assert!((got - w).abs() <= 0.05, "hdr {hdr} face {face} channel {k}: {got} vs {w}");
            }
            let small = hp_vtf::raw_level(&data, 4, 0, face).expect("smallest mip");
            assert_eq!((small.width, small.height), (1, 1));
        }
    }
    let mut faces = colored_faces(4);
    faces[0] = (0..16).map(|i| Vec3A::splat(if i % 2 == 0 { 1.0 } else { 0.0 })).collect();
    let mips = vtf::mips(&faces[0], 4);
    assert_eq!(mips.len(), 3);
    assert!((mips[2][0].x - 0.5).abs() < 1e-6, "mips average in linear light");
}

#[test]
fn sizes_follow_the_cubemap_lump() {
    assert_eq!(super::pixels(0), super::DEFAULT_SIZE);
    assert_eq!(super::pixels(6), 32);
    assert_eq!(super::pixels(9), 256);
    assert_eq!(super::pixels(1), 4);
    assert_eq!(super::pixels(30), 1024);
    let (jobs, shrunk) = super::plan("gm_x", &[([0, 0, 64], 0), ([0, 0, 64], 6), ([512, 0, 64], 11)], [true, true]);
    assert_eq!(jobs.len(), 3, "one job per origin and the default cubemap");
    assert!(jobs.iter().any(|j| j.name == "materials/maps/gm_x/c0_0_64" && j.size == 32));
    assert!(jobs.iter().any(|j| j.name == "materials/maps/gm_x/cubemapdefault" && j.origin.is_none()));
    assert!(!shrunk);
    let many: Vec<([i32; 3], i32)> = (0..64).map(|i| ([i * 64, 0, 0], 11)).collect();
    let (jobs, shrunk) = super::plan("gm_x", &many, [true, true]);
    assert!(shrunk && jobs.iter().all(|j| j.size < 1024), "the pakfile budget shrinks the largest");
}

struct Files;

fn texture(rgb: [u8; 3], size: u32) -> Vec<u8> {
    let pixels = (0..size * size).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect();
    let img = hp_vtf::RgbaImage { width: size, height: size, pixels };
    let opts = hp_vtf::EncodeOptions { format: hp_vtf::EncodeFormat::Bgra8888, mipmaps: false, ..Default::default() };
    hp_vtf::encode(&img, &opts).expect("texture")
}

impl Files {
    fn file(&self, path: &str) -> Option<Vec<u8>> {
        let p = path.to_ascii_lowercase();
        let sides = ["rt", "lf", "bk", "ft", "up", "dn"];
        if let Some(side) = p.strip_prefix("materials/skybox/sky_day01_01") {
            let i = sides.iter().position(|s| side.starts_with(s))?;
            if side.ends_with(".vmt") {
                return Some(
                    format!("UnlitGeneric {{ \"$basetexture\" \"skybox/sky_day01_01{}\" }}", sides[i]).into_bytes(),
                );
            }
            return Some(texture(SKY_COLORS[i], 8));
        }
        match p.as_str() {
            "materials/dev/shiny.vmt" => {
                Some(b"LightmappedGeneric { \"$basetexture\" \"dev/shiny\" \"$envmap\" \"env_cubemap\" }".to_vec())
            }
            "materials/dev/shiny.vtf" => Some(texture([200, 200, 200], 8)),
            _ => None,
        }
    }
}

impl MaterialSource for Files {
    fn facts(&self, name: &str) -> MaterialFacts {
        let mut f = facts_by_name(name);
        f.reflectivity = [0.5; 3];
        if name.eq_ignore_ascii_case("dev/shiny") {
            f.envmap = Some("env_cubemap".to_owned());
        }
        f
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.file(path)
    }
}

impl crate::props::ModelSource for Files {
    fn bounds(&self, _path: &str) -> Option<(DVec3, DVec3)> {
        None
    }

    fn read_file(&self, path: &str) -> Option<Vec<u8>> {
        self.file(path)
    }
}

fn face_mean(data: &[u8], face: u32) -> Vec3A {
    let img = hp_vtf::raw_level(data, 0, 0, face).expect("level").to_rgba8().expect("rgba");
    let mut sum = Vec3A::ZERO;
    for p in img.pixels.chunks_exact(4) {
        sum += Vec3A::new(f32::from(p[0]), f32::from(p[1]), f32::from(p[2]));
    }
    sum / (img.pixels.len() / 4) as f32
}

#[test]
fn compiled_map_gets_ray_traced_cubemaps_in_its_pakfile() {
    let cube = "entity\n{\n\t\"id\" \"8000\"\n\t\"classname\" \"env_cubemap\"\n\t\"origin\" \"128 128 96\"\n\t\"cubemapsize\" \"4\"\n}\n";
    let light = crate::rad::tests::light_entity(500, "64 128 200", "");
    let text = crate::rad::tests::room("DEV/SHINY", &format!("{light}{cube}"));
    let map = hp_vmf::parse(&text).expect("vmf");
    let opts = crate::CompileOptions { map_name: Some("gm_reflect".to_owned()), ..Default::default() };
    let compiled = crate::compile(&map, &Files, &crate::props::NoModels, &opts, &mut |_| {}).expect("compile");
    let rt = RayTracing {
        device: DevicePreference::Cpu,
        min_samples: Some(16),
        max_samples: Some(32),
        effects: Effects { reflections: Some(true), ..Effects::none() },
        ..RayTracing::with_quality(Quality::Draft)
    };
    let rad = RadOptions {
        bounces: 2,
        quality: RadQuality::Draft,
        threads: Some(2),
        raytracing: Some(rt.clone()),
        ..RadOptions::default()
    };
    let mut lines = Vec::new();
    let out =
        crate::rad::light_bsp_with(&compiled.bsp, &rad, &Files, &mut |l| lines.push(l.to_owned())).expect("light");
    assert!(lines.iter().any(|l| l.contains("cubemaps: 1 env_cubemap rendered (8 px)")), "{lines:?}");
    assert!(crate::validate::validate(&out).is_empty(), "{:?}", crate::validate::validate(&out));
    let bsp = hp_bsp::Bsp::from_vec(out.clone()).expect("bsp");
    let pak = bsp.pakfile().expect("pak");
    let read = |n: &str| {
        pak.read_name(n).ok().flatten().map(|d| d.into_owned()).unwrap_or_else(|| {
            panic!("{n} not in {:?}", pak.entries().iter().map(|e| e.name.as_str()).collect::<Vec<_>>())
        })
    };
    let vmt = String::from_utf8_lossy(&read("materials/maps/gm_reflect/dev/shiny_128_128_96.vmt")).to_ascii_lowercase();
    assert!(vmt.contains("maps/gm_reflect/c128_128_96"), "{vmt}");
    for (name, hdr) in [("c128_128_96.vtf", false), ("c128_128_96.hdr.vtf", true)] {
        let data = read(&format!("materials/maps/gm_reflect/{name}"));
        let info = hp_vtf::read_info(&data).expect("vtf");
        assert_eq!((info.width, info.faces, info.mip_count), (8, 7, 4), "{name}");
        assert_eq!(info.format.raw(), if hdr { vtf::FORMAT_RGBA16161616F } else { vtf::FORMAT_DXT1 });
        let floor = face_mean(&data, 5);
        let ceiling_side = face_mean(&data, 0);
        assert!(floor.max_element() > 4.0, "{name}: the lit floor shows in the bottom face: {floor}");
        assert!(ceiling_side.max_element() > 1.0, "{name}: {ceiling_side}");
    }
    for (name, hdr) in [("cubemapdefault.vtf", false), ("cubemapdefault.hdr.vtf", true)] {
        let data = read(&format!("materials/maps/gm_reflect/{name}"));
        #[allow(clippy::needless_range_loop)]
        for face in 0..6 {
            let c = face_mean(&data, face as u32);
            let want = Vec3A::new(SKY_COLORS[face][0] as f32, SKY_COLORS[face][1] as f32, SKY_COLORS[face][2] as f32);
            let dominant =
                |v: Vec3A| [v.x > v.max_element() * 0.6, v.y > v.max_element() * 0.6, v.z > v.max_element() * 0.6];
            assert_eq!(dominant(c), dominant(want), "{name} hdr {hdr} face {face} shows sky face {face}: {c}");
        }
    }
    let again = crate::rad::light_bsp_with(&compiled.bsp, &rad, &Files, &mut |_| {}).expect("again");
    assert_eq!(again, out, "the same seed gives the same cubemaps");
    let off = RadOptions {
        raytracing: Some(RayTracing { effects: Effects { reflections: None, ..rt.effects }, ..rt }),
        ..rad
    };
    let mut lines = Vec::new();
    let plain =
        crate::rad::light_bsp_with(&compiled.bsp, &off, &Files, &mut |l| lines.push(l.to_owned())).expect("off");
    assert!(!lines.iter().any(|l| l.contains("env_cubemap rendered")), "draft leaves them to buildcubemaps");
    let pak = hp_bsp::Bsp::from_vec(plain).expect("bsp");
    let placeholder = pak
        .pakfile()
        .expect("pak")
        .read_name("materials/maps/gm_reflect/c128_128_96.vtf")
        .ok()
        .flatten()
        .map(|d| d.into_owned())
        .expect("placeholder");
    assert!(face_mean(&placeholder, 5).max_element() < 1.0, "vbsp placeholders stay black");
}

#[test]
fn reflections_are_on_by_default_only_for_final_and_optimal() {
    let with = |rt: Option<RayTracing>, cubemaps: Option<bool>| RadOptions {
        raytracing: rt,
        cubemaps,
        ..RadOptions::default()
    };
    for (q, on) in [(Quality::Draft, false), (Quality::Normal, false), (Quality::Final, true), (Quality::Optimal, true)]
    {
        assert_eq!(with(Some(RayTracing::with_quality(q)), None).builds_cubemaps(), on, "{q:?}");
    }
    let mut rt = RayTracing::with_quality(Quality::Normal);
    rt.effects.reflections = Some(true);
    assert!(with(Some(rt.clone()), None).builds_cubemaps());
    rt.quality = Quality::Final;
    rt.effects.reflections = Some(false);
    assert!(!with(Some(rt.clone()), None).builds_cubemaps());
    assert!(with(Some(rt), Some(true)).builds_cubemaps(), "-buildcubemaps wins over -rtnoreflections");
    let none = RayTracing { effects: Effects::none(), ..RayTracing::with_quality(Quality::Final) };
    assert!(!with(Some(none), None).builds_cubemaps());
    assert!(!with(None, None).builds_cubemaps(), "the classic compiler leaves them to buildcubemaps by default");
    assert!(with(None, Some(true)).builds_cubemaps());
    assert!(!with(Some(RayTracing::with_quality(Quality::Final)), Some(false)).builds_cubemaps());
}

#[test]
fn classic_lighting_builds_cubemaps_without_ray_tracing() {
    let cube = "entity
{
	\"id\" \"8000\"
	\"classname\" \"env_cubemap\"
	\"origin\" \"128 128 96\"
	\"cubemapsize\" \"4\"
}
";
    let light = crate::rad::tests::light_entity(500, "64 128 200", "");
    let text = crate::rad::tests::room("DEV/SHINY", &format!("{light}{cube}"));
    let map = hp_vmf::parse(&text).expect("vmf");
    let opts = crate::CompileOptions { map_name: Some("gm_classic".to_owned()), ..Default::default() };
    let compiled = crate::compile(&map, &Files, &crate::props::NoModels, &opts, &mut |_| {}).expect("compile");
    let rad = RadOptions {
        bounces: 2,
        quality: RadQuality::Draft,
        threads: Some(2),
        hdr: false,
        cubemaps: Some(true),
        ..RadOptions::default()
    };
    let mut lines = Vec::new();
    let out =
        crate::rad::light_bsp_with(&compiled.bsp, &rad, &Files, &mut |l| lines.push(l.to_owned())).expect("light");
    assert!(lines.iter().any(|l| l.contains("cubemaps: 1 env_cubemap rendered (8 px)")), "{lines:?}");
    let bsp = hp_bsp::Bsp::from_vec(out.clone()).expect("bsp");
    let pak = bsp.pakfile().expect("pak");
    let data = pak.read_name("materials/maps/gm_classic/c128_128_96.vtf").ok().flatten().expect("cubemap").into_owned();
    assert!(face_mean(&data, 5).max_element() > 4.0, "the lit floor shows in the bottom face");
    let again = crate::rad::build_cubemaps(out, &Files, false, &mut |_| {}).expect("post step");
    assert!(crate::validate::validate(&again).is_empty());
}

#[test]
fn painted_sky_faces_line_up_with_the_skybox_layout() {
    for face in 0..6 {
        for (s, t) in [(-0.6f32, 0.4f32), (0.25, -0.8), (0.0, 0.0)] {
            let (back, u, v) = sky_lookup(super::sky::sky_face_dir(face, s, t));
            assert_eq!(back, face);
            assert!((u * 2.0 - 1.0 - s).abs() < 1e-5 && (v * 2.0 - 1.0 - t).abs() < 1e-5, "{face} {s} {t}");
        }
    }
    let mut paint = super::sky::Paint { dusk_intensity: 0.0, sun_size: 0.0, ..Default::default() };
    paint.top = Vec3A::new(0.0, 0.0, 1.0);
    paint.bottom = Vec3A::new(1.0, 0.0, 0.0);
    let up = super::sky::color(&paint, Vec3A::Z, true, None);
    let side = super::sky::color(&paint, Vec3A::X, false, None);
    assert!((up - Vec3A::new(0.0, 0.0, 0.66)).length() < 1e-4, "{up}");
    assert!((side - Vec3A::X).length() < 1e-4, "{side}");
    let ents = [hp_bsp::Entity {
        pairs: vec![
            ("classname".into(), "env_skypaint".into()),
            ("topcolor".into(), "0.1 0.2 0.3".into()),
            ("hdrscale".into(), "0.5".into()),
        ],
    }];
    let p = super::sky::paint_of(&ents);
    assert_eq!((p.top, p.hdr_scale), (Vec3A::new(0.1, 0.2, 0.3), 0.5));
}
