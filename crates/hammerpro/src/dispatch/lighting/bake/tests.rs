use std::collections::{HashMap, HashSet};

use hp_render::lightmap::face_flags;

use super::*;

static LEVEL: AtomicU8 = AtomicU8::new(1);

pub(in crate::dispatch::lighting) fn solid(id: u32, side0: u32, lo: [f64; 3], hi: [f64; 3]) -> String {
    let ([x0, y0, z0], [x1, y1, z1]) = (lo, hi);
    let sides = [
        (format!("({x0} {y1} {z1}) ({x1} {y1} {z1}) ({x1} {y0} {z1})"), "[1 0 0 0] 0.25", "[0 -1 0 0] 0.25"),
        (format!("({x0} {y0} {z0}) ({x1} {y0} {z0}) ({x1} {y1} {z0})"), "[1 0 0 0] 0.25", "[0 -1 0 0] 0.25"),
        (format!("({x0} {y1} {z1}) ({x0} {y0} {z1}) ({x0} {y0} {z0})"), "[0 1 0 0] 0.25", "[0 0 -1 0] 0.25"),
        (format!("({x1} {y1} {z0}) ({x1} {y0} {z0}) ({x1} {y0} {z1})"), "[0 1 0 0] 0.25", "[0 0 -1 0] 0.25"),
        (format!("({x1} {y1} {z1}) ({x0} {y1} {z1}) ({x0} {y1} {z0})"), "[1 0 0 0] 0.25", "[0 0 -1 0] 0.25"),
        (format!("({x1} {y0} {z0}) ({x0} {y0} {z0}) ({x0} {y0} {z1})"), "[1 0 0 0] 0.25", "[0 0 -1 0] 0.25"),
    ];
    let mut out = format!("solid {{ \"id\" \"{id}\"\n");
    for (k, (plane, u, v)) in sides.iter().enumerate() {
        out += &format!(
            "side {{ \"id\" \"{}\" \"plane\" \"{plane}\" \"material\" \"DEV/DEV_MEASUREGENERIC01B\" \"uaxis\" \"{u}\" \"vaxis\" \"{v}\" \"lightmapscale\" \"16\" }}\n",
            side0 + k as u32
        );
    }
    out + "}\n"
}

const WALLS: [([f64; 3], [f64; 3]); 6] = [
    ([-16.0, -16.0, -16.0], [272.0, 272.0, 0.0]),
    ([-16.0, -16.0, 256.0], [272.0, 272.0, 272.0]),
    ([-16.0, -16.0, 0.0], [0.0, 272.0, 256.0]),
    ([256.0, -16.0, 0.0], [272.0, 272.0, 256.0]),
    ([0.0, -16.0, 0.0], [256.0, 0.0, 256.0]),
    ([0.0, 256.0, 0.0], [256.0, 272.0, 256.0]),
];

fn room_text(lights: &str) -> (String, HashMap<u32, SolidInfo>) {
    let mut text = String::from("world { \"id\" \"1\" \"classname\" \"worldspawn\"\n");
    let mut solids = HashMap::new();
    for (i, (lo, hi)) in WALLS.iter().enumerate() {
        let id = 2 + i as u32;
        let side0 = if i < 3 { 10 + 6 * i as u32 } else { 65_536 + 10 + 6 * (i as u32 - 3) };
        text += &solid(id, side0, *lo, *hi);
        let info = SolidInfo {
            lo: glam::Vec3::from_array(lo.map(|v| v as f32)),
            hi: glam::Vec3::from_array(hi.map(|v| v as f32)),
            generation: u64::from(id),
        };
        solids.insert(id, info);
    }
    text += "}\n";
    text += lights;
    text += "entity { \"id\" \"21\" \"classname\" \"info_player_start\" \"origin\" \"128 128 32\" }\n";
    (text, solids)
}

fn light(id: u32, origin: &str, color: &str) -> String {
    format!("entity {{ \"id\" \"{id}\" \"classname\" \"light\" \"origin\" \"{origin}\" \"_light\" \"{color}\" }}\n")
}

fn job(id: u64, map: Option<hp_vmf::Map>, solids: HashMap<u32, SolidInfo>) -> Job {
    Job {
        id,
        geometry_key: 1,
        map,
        points: None,
        solids,
        fs: None,
        hdr: true,
        quality: RadQuality::Draft,
        bounces: 2,
        prop_shadows: true,
        prop_lighting: true,
        threads: (2, 2),
        layer_size: 256,
        max_layers: 4,
        coarsen: 1.0,
        lit: false,
        tuning: child::Tuning::default(),
        exe: None,
        focus: None,
        stream: false,
        shown: None,
        models: true,
        exact: false,
        quiet: false,
        priority: BakePriority::BelowNormal,
    }
}

struct Harness {
    wanted: AtomicU64,
    out: Sender<Msg>,
    rx: Receiver<Msg>,
    pipe: Pipeline,
}

impl Harness {
    fn new() -> Self {
        let (out, rx) = crossbeam_channel::unbounded();
        Harness { wanted: AtomicU64::new(0), out, rx, pipe: Pipeline::default() }
    }

    fn run(&mut self, job: Job) -> Outcome {
        let id = job.id;
        self.wanted.store(id, Ordering::Release);
        let wake = || {};
        let ctx = Ctx {
            id,
            wanted: &self.wanted,
            out: &self.out,
            wake: &wake,
            start: Instant::now(),
            faces: 0,
            quiet: false,
            priority: &LEVEL,
            woke: AtomicU64::new(0),
        };
        run(job, &ctx, &mut self.pipe)
    }

    fn baked(&mut self, job: Job) -> Box<Baked> {
        match self.run(job) {
            Outcome::Baked(b) => b,
            Outcome::Cancelled => panic!("cancelled"),
            Outcome::Failed(e) => panic!("failed: {e}"),
            Outcome::NeedsMap => panic!("needs the map"),
        }
    }

    fn models(&mut self, id: u64) -> Box<ModelLight> {
        start_models(&mut self.pipe, id);
        let deadline = Instant::now() + Duration::from_secs(300);
        loop {
            let wake = || {};
            pump(&mut self.pipe, &self.out, &wake);
            if let Some(m) =
                self.rx.try_iter().find_map(|m| if let Msg::Models { models, .. } = m { Some(models) } else { None })
            {
                return m;
            }
            assert!(Instant::now() < deadline, "model lighting");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn texels_by_bsp(b: &Baked) -> HashMap<u32, Vec<u32>> {
    b.faces
        .iter()
        .filter(|f| f.texels != u32::MAX)
        .filter_map(|f| {
            let n = f.rect.map(|r| (r.w * r.h) as usize)?;
            Some((f.bsp, b.texels.get(f.texels as usize..f.texels as usize + n)?.to_vec()))
        })
        .collect()
}

fn lum(t: u32) -> f32 {
    let c = hp_light::pack::unpack_rg11b10(t);
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

#[test]
fn vmf_text_round_trip_compiles_identically() {
    let (text, _) = room_text("");
    let map = hp_vmf::parse(&text).expect("map");
    let fs = hp_fs::GameFs::empty();
    let never = || false;
    let compile = |m: &hp_vmf::Map| compile_map(m, &fs, 2, &never, &mut |_| {}, false).ok().expect("compile");
    let local = compile(&map);
    let restored = hp_vmf::parse(&hp_vmf::write(&map)).expect("restore map");
    let again = compile(&restored);
    assert_eq!(again.bsp, local.bsp, "the lighting process receives identical compile inputs");
    assert_eq!(again.leaked, local.leaked);
    assert_eq!(again.warnings, local.warnings);
}

#[test]
fn a_room_is_compiled_lit_and_bound_to_its_solids() {
    let (text, solids) = room_text(&light(20, "128 128 128", "255 255 255 300"));
    let map = hp_vmf::parse(&text).expect("parse");
    let mut h = Harness::new();
    let baked = h.baked(job(7, Some(map), solids));
    assert!(!baked.patch && baked.layout > 0);
    assert_eq!(baked.unbound, 0, "every face found its solid");
    assert!(baked.faces.len() >= 6, "{} faces", baked.faces.len());
    assert!(baked.faces.iter().all(|f| (2..8).contains(&f.solid) && f.generation == u64::from(f.solid)));
    assert!(baked.faces.iter().all(|f| f.rect.is_some() && f.texels != u32::MAX));
    assert!(baked.faces.iter().enumerate().all(|(i, f)| f.id == i as u32));
    let walls_seen: HashSet<u32> = baked.faces.iter().map(|f| f.solid).collect();
    assert_eq!(walls_seen.len(), 6, "the inner face of every wall");
    let light: Vec<f32> = baked.texels.iter().map(|t| hp_light::pack::unpack_rg11b10(*t)[0]).collect();
    let max = light.iter().copied().fold(0.0f32, f32::max);
    assert!(max > 0.1 && max < 100.0, "{max}");
    assert!(light.iter().all(|v| v.is_finite()));
    let verts: Vec<_> = baked.blocks.iter().flat_map(|b| b.batches.iter().flat_map(|x| x.1.iter())).collect();
    assert!(!verts.is_empty());
    assert!(verts.iter().all(|v| v.pos.iter().all(|c| (-0.5..=256.5).contains(c))));
    assert!(verts.iter().all(|v| (v.face as usize) < baked.faces.len()));
    let phases: Vec<BakePhase> =
        h.rx.try_iter().filter_map(|m| if let Msg::Progress { phase, .. } = m { Some(phase) } else { None }).collect();
    assert!(phases.contains(&BakePhase::Compiling) && phases.contains(&BakePhase::Direct), "{phases:?}");
    let models = h.models(7);
    let props = models.props.as_ref().expect("model lighting");
    let lit = props.model_lighting([128.0, 128.0, 64.0]);
    assert!(lit.lights.iter().any(|l| l.origin == [128.0, 128.0, 128.0]), "{:?}", lit.lights);
    assert!(lit.ambient.iter().flatten().any(|v| *v > 0.0), "ambient cube {:?}", lit.ambient);
    let volume = models.indirect.as_ref().expect("the light probes of the bake");
    assert!(volume.cells() > 20 && volume.spacing >= 32.0, "{} cells of {}", volume.cells(), volume.spacing);

    let mut lit_job = job(9, Some(hp_vmf::parse(&text).expect("parse")), HashMap::new());
    lit_job.lit = true;
    lit_job.tuning = child::Tuning::default();
    let only = h.baked(lit_job);
    assert!(only.faces.is_empty() && only.texels.is_empty() && only.blocks.is_empty(), "no atlas for the Lit mode");
    let own = h.models(9).indirect.clone().expect("the light probes");
    assert_eq!(own.cells(), volume.cells(), "the same probes as the full bake");

    let cancelled = job(8, Some(hp_vmf::parse(&text).expect("parse")), HashMap::new());
    h.wanted.store(0, Ordering::Release);
    let wake = || {};
    let ctx = Ctx {
        id: 8,
        wanted: &h.wanted,
        out: &h.out,
        wake: &wake,
        start: Instant::now(),
        faces: 0,
        quiet: false,
        priority: &LEVEL,
        woke: AtomicU64::new(0),
    };
    let r = catch_unwind(AssertUnwindSafe(|| run(cancelled, &ctx, &mut h.pipe)));
    assert!(matches!(r, Err(_) | Ok(Outcome::Cancelled)));
}

#[test]
fn light_edits_patch_the_shown_atlas_without_compiling() {
    let lights = light(20, "64 128 128", "255 255 255 300") + &light(30, "200 140 100", "255 200 150 250");
    let (text, solids) = room_text(&lights);
    let map = hp_vmf::parse(&text).expect("parse");
    let mut h = Harness::new();
    let first = h.baked(job(1, Some(map.clone()), solids.clone()));
    let before = texels_by_bsp(&first);
    let mut moved = map.clone();
    let lamp = moved.entities.iter_mut().find(|e| e.id == 20).expect("light");
    lamp.set("origin", "90 60 180");
    lamp.set("_light", "255 40 40 500");
    let mut edit = job(2, None, HashMap::new());
    edit.points = Some(point_entities(&moved));
    edit.shown = Some(first.layout);
    let patched = h.baked(edit);
    assert!(patched.patch, "the shown atlas is patched");
    assert_eq!(patched.layout, first.layout);
    assert!(patched.blocks.is_empty(), "no new geometry");
    assert!(!patched.faces.is_empty() && patched.faces.len() <= first.faces.len());
    assert!(patched.stats.changed_lights >= 1 && !patched.stats.full);
    let after = texels_by_bsp(&patched);
    assert!(after.iter().any(|(k, v)| before.get(k) != Some(v)), "the edit changes the lightmaps");
    for f in &patched.faces {
        assert_eq!(first.faces[f.id as usize].bsp, f.bsp, "renderer ids follow the shown atlas");
        assert_eq!(first.faces[f.id as usize].rect, f.rect);
    }

    let mut fresh = Harness::new();
    let reference = texels_by_bsp(&fresh.baked(job(5, Some(moved.clone()), solids.clone())));
    let mut shown = before.clone();
    shown.extend(after);
    let (mut diff, mut peak) = (0.0f32, 0.0f32);
    for (k, v) in &reference {
        for (a, b) in v.iter().zip(&shown[k]) {
            diff = diff.max((lum(*a) - lum(*b)).abs());
            peak = peak.max(lum(*a));
        }
    }
    assert!(diff <= peak * 0.03 + 0.01, "patched lightmaps match a fresh bake: {diff} of {peak}");

    let mut lost = job(3, None, HashMap::new());
    lost.points = Some(point_entities(&moved));
    lost.shown = None;
    let again = h.baked(lost);
    assert!(!again.patch && again.layout != first.layout, "a lost atlas comes back whole");
    assert_eq!(again.faces.len(), first.faces.len());
    assert!(!again.blocks.is_empty());

    let mut unknown = Harness::new();
    let mut cold = job(4, None, HashMap::new());
    cold.points = Some(point_entities(&moved));
    assert!(matches!(unknown.run(cold), Outcome::NeedsMap), "a cold lighting process needs the map");
}

#[test]
fn model_lighting_gives_way_to_the_next_edit_and_keeps_the_process_warm() {
    let (text, solids) = room_text(&light(20, "64 128 128", "255 255 255 300"));
    let map = hp_vmf::parse(&text).expect("parse");
    let mut h = Harness::new();
    let first = h.baked(job(1, Some(map.clone()), solids));
    start_models(&mut h.pipe, 1);
    let mut moved = map;
    moved.entities.iter_mut().find(|e| e.id == 20).expect("light").set("origin", "180 100 90");
    let mut edit = job(2, None, HashMap::new());
    edit.points = Some(point_entities(&moved));
    edit.shown = Some(first.layout);
    let patched = h.baked(edit);
    assert!(patched.patch && patched.stats.changed_lights == 1, "the edit is relit while the props wait");
    let models = h.models(2);
    assert!(models.props.is_some(), "the props are lit for the latest lights");
    assert!(h.pipe.geometry.is_some(), "the lighting process kept the geometry");
}

#[test]
fn big_first_bakes_show_the_atlas_at_once_then_stream_the_nearest_lightmaps() {
    let (text, solids) = room_text(&light(20, "64 128 128", "255 255 255 300"));
    let mut h = Harness::new();
    let mut first = job(1, Some(hp_vmf::parse(&text).expect("parse")), solids);
    first.stream = true;
    first.focus = Some([0.0, 0.0, 0.0]);
    let done = h.baked(first);
    let partials: Vec<Box<Baked>> =
        h.rx.try_iter().filter_map(|m| if let Msg::Partial { baked, .. } = m { Some(baked) } else { None }).collect();
    assert!(partials.len() >= 2, "{} partial results", partials.len());
    let layout = &partials[0];
    assert!(!layout.patch && !layout.blocks.is_empty(), "the geometry comes first");
    assert!(layout.faces.iter().all(|f| f.texels != u32::MAX), "unlit faces show a neutral light meanwhile");
    assert!(partials[1..].iter().all(|p| p.patch && p.layout == layout.layout));
    assert!(done.patch && done.layout == layout.layout, "the final lightmaps patch the streamed atlas");
    assert_eq!(done.faces.len(), layout.faces.len());
}

#[test]
fn baked_light_matches_the_lit_preview() {
    let walls = [
        ([-16.0, -16.0, -16.0], [528.0, 528.0, 0.0]),
        ([-16.0, -16.0, 512.0], [528.0, 528.0, 528.0]),
        ([-16.0, -16.0, 0.0], [0.0, 528.0, 512.0]),
        ([512.0, -16.0, 0.0], [528.0, 528.0, 512.0]),
        ([0.0, -16.0, 0.0], [512.0, 0.0, 512.0]),
        ([0.0, 512.0, 0.0], [512.0, 528.0, 512.0]),
    ];
    let room = |sky: bool| {
        let mut text = String::from("world { \"id\" \"1\" \"classname\" \"worldspawn\"\n");
        for (i, (lo, hi)) in walls.iter().enumerate() {
            let s = solid(2 + i as u32, 10 + 6 * i as u32, *lo, *hi);
            text += &if sky && i == 1 { s.replace("DEV/DEV_MEASUREGENERIC01B", "TOOLS/TOOLSSKYBOX") } else { s };
        }
        text + "}\n"
    };
    let point = room(false)
        + "entity { \"id\" \"20\" \"classname\" \"light\" \"origin\" \"256 256 100\" \"_light\" \"255 255 255 255\" \"_quadratic_attn\" \"1\" }\n";
    let sun = room(true)
        + "entity { \"id\" \"20\" \"classname\" \"light_environment\" \"origin\" \"256 256 100\" \"_light\" \"255 255 255 255\" \"_ambient\" \"0 0 0 0\" \"pitch\" \"-90\" \"angles\" \"-90 0 0\" }\n";
    let solids: HashMap<u32, SolidInfo> = walls
        .iter()
        .enumerate()
        .map(|(i, (lo, hi))| {
            let info = SolidInfo {
                lo: glam::Vec3::from_array(lo.map(|v| v as f32)),
                hi: glam::Vec3::from_array(hi.map(|v| v as f32)),
                generation: 1,
            };
            (2 + i as u32, info)
        })
        .collect();
    for (text, hdr) in [(&point, true), (&point, false), (&sun, true), (&sun, false)] {
        let mut j = job(3, Some(hp_vmf::parse(text).expect("parse")), solids.clone());
        j.hdr = hdr;
        j.bounces = 0;
        j.prop_shadows = false;
        j.prop_lighting = false;
        j.layer_size = 512;
        j.models = false;
        let baked = Harness::new().baked(j);
        assert!(!baked.faces.is_empty(), "the faces are bound to their solids");
        let max = baked.texels.iter().map(|t| hp_light::pack::unpack_rg11b10(*t)[0]).fold(0.0f32, f32::max);
        let what = if std::ptr::eq(text, &sun) { "sun" } else { "point light" };
        assert!((max - 1.0).abs() < 0.12, "{what}, hdr {hdr}: the floor at {max}, the Lit preview at 1");
    }
}

#[test]
fn bumped_faces_bring_their_bumped_lightmaps_and_blends_their_weights() {
    let dir = std::env::temp_dir().join(format!("hp-light-bump-{}", std::process::id()));
    let mats = dir.join("materials/test");
    std::fs::create_dir_all(&mats).expect("dir");
    std::fs::write(
        mats.join("bumpy.vmt"),
        r#"LightmappedGeneric { "$basetexture" "test/bumpy" "$bumpmap" "test/bumpy_normal" }"#,
    )
    .expect("vmt");
    std::fs::write(
        mats.join("blend4.vmt"),
        r#"Lightmapped_4WayBlend { "$basetexture" "test/a" "$basetexture2" "test/b" "$basetexture3" "test/c"
           "$basetexture4" "test/d" "$bumpmap" "test/a_normal" "$bumpmap2" "test/b_normal" }"#,
    )
    .expect("vmt");
    let fs = hp_fs::GameFs::empty();
    fs.add_game_dir(&dir).expect("game folder");
    let walls = [
        ([-16.0, -16.0, -16.0], [272.0, 272.0, 0.0]),
        ([-16.0, -16.0, 256.0], [272.0, 272.0, 272.0]),
        ([-16.0, -16.0, 0.0], [0.0, 272.0, 256.0]),
        ([256.0, -16.0, 0.0], [272.0, 272.0, 256.0]),
        ([0.0, -16.0, 0.0], [256.0, 0.0, 256.0]),
        ([0.0, 256.0, 0.0], [256.0, 272.0, 256.0]),
        ([64.0, 64.0, 0.0], [192.0, 192.0, 8.0]),
    ];
    let mut text = String::from("world { \"id\" \"1\" \"classname\" \"worldspawn\"\n");
    let mut solids = HashMap::new();
    for (i, (lo, hi)) in walls.iter().enumerate() {
        let id = 2 + i as u32;
        let mut s = solid(id, 10 + 6 * i as u32, *lo, *hi);
        if i == 0 {
            s = s.replace("DEV/DEV_MEASUREGENERIC01B", "TEST/BUMPY");
        }
        if i == 6 {
            s = s.replace("DEV/DEV_MEASUREGENERIC01B", "TEST/BLEND4").replacen(
                "\"lightmapscale\" \"16\" }",
                "\"lightmapscale\" \"16\" dispinfo { \"power\" \"2\" \"startposition\" \"[64 64 8]\" } }",
                1,
            );
        }
        text += &s;
        let info = SolidInfo {
            lo: glam::Vec3::from_array(lo.map(|v| v as f32)),
            hi: glam::Vec3::from_array(hi.map(|v| v as f32)),
            generation: u64::from(id),
        };
        solids.insert(id, info);
    }
    text += "}\n";
    text +=
        "entity { \"id\" \"30\" \"classname\" \"light\" \"origin\" \"128 40 128\" \"_light\" \"255 255 255 300\" }\n";
    text += "entity { \"id\" \"31\" \"classname\" \"info_player_start\" \"origin\" \"128 128 32\" }\n";
    let mut map = hp_vmf::parse(&text).expect("parse");
    let disp_solid = map.world.solids.iter_mut().find(|s| s.id == 8).expect("block");
    let disp = disp_solid.sides[0].disp.as_deref_mut().expect("displacement");
    let mut mb = hp_vmf::multiblend::MultiBlend::new(25);
    for (i, w) in mb.weights.iter_mut().enumerate() {
        let t = i as f64 / 24.0;
        *w = [1.0 - t, t, 0.0, 0.0];
    }
    assert!(mb.write_to(disp));
    let editor = hp_editor::Document::from_map(map.clone());
    let positions: Vec<glam::DVec3> =
        editor.scene().solid(8).and_then(|n| n.disp_mesh(0)).expect("editor displacement").positions.clone();
    let mut j = job(9, Some(map), solids);
    j.fs = Some(fs);
    j.bounces = 1;
    j.prop_shadows = false;
    j.prop_lighting = false;
    j.layer_size = 512;
    j.models = false;
    let baked = Harness::new().baked(j);
    let _ = std::fs::remove_dir_all(&dir);
    let of = |solid: u32| baked.faces.iter().filter(move |f| f.solid == solid);
    for solid in [2, 8] {
        let f = of(solid).find(|f| f.rect.is_some()).expect("lit face");
        assert_eq!(f.images, 4, "solid {solid}");
        let r = f.rect.expect("rect");
        assert_eq!(r.w % 4, 0);
        let n = (r.w * r.h) as usize;
        let texels = &baked.texels[f.texels as usize..f.texels as usize + n];
        let lum = |k: usize| -> f32 {
            let per = n / 4;
            texels[k * per..(k + 1) * per].iter().map(|t| hp_light::pack::unpack_rg11b10(*t)[0]).sum::<f32>()
        };
        assert!((1..4).all(|k| lum(k) > 0.0), "lit along every bump direction");
        let average = (lum(1) + lum(2) + lum(3)) / 3.0;
        assert!(
            (average - lum(0)).abs() <= 0.03 * lum(0),
            "bump lightmaps scaled to the flat one like the compiled view: {average} vs {}",
            lum(0)
        );
    }
    assert!(of(4).all(|f| f.images == 1), "plain walls keep one lightmap");
    let face = baked.faces.iter().position(|f| f.solid == 8 && f.flags & face_flags::DISP != 0).expect("disp face");
    let verts: Vec<_> = baked
        .blocks
        .iter()
        .flat_map(|b| b.batches.iter().flat_map(|x| x.1.iter()))
        .filter(|v| v.face as usize == face)
        .collect();
    assert_eq!(verts.len(), 25);
    for v in verts {
        let p = glam::DVec3::new(f64::from(v.pos[0]), f64::from(v.pos[1]), f64::from(v.pos[2]));
        let i = positions.iter().position(|q| q.distance(p) < 0.1).expect("editor vertex");
        let t = ((i as f64 / 24.0) * 1e6).round() as f32 / 1e6;
        let want = hp_render::lightmap::pack_blend([1.0 - t, t, 0.0, 0.0]);
        assert_eq!(v.blend, want, "vertex {i} at {p}");
    }
}

#[test]
fn the_atlas_is_as_small_as_the_lightmaps() {
    assert_eq!(atlas_size(&[], 2048), MIN_ATLAS);
    assert_eq!(atlas_size(&[(16, 16); 4], 2048), MIN_ATLAS);
    assert_eq!(atlas_size(&[(300, 20)], 2048), 512, "as wide as the widest face");
    assert_eq!(atlas_size(&[(32, 32); 2000], 2048), 2048);
    assert_eq!(atlas_size(&[(32, 32); 100_000], 2048), 2048, "more layers past the largest size");
    let sizes = [(40, 30); 700];
    let mut atlas = Atlas::new(atlas_size(&sizes, 2048), 64);
    assert!(atlas.alloc_many(&sizes).iter().all(Option::is_some));
    assert!(atlas.layers() <= 2, "{} layers of {}", atlas.layers(), atlas.size());
}

#[test]
fn hidden_objects_are_left_out_of_the_compiled_copy() {
    let text = r#"world { "id" "1" "classname" "worldspawn"
  solid { "id" "2" side { "id" "1" "plane" "(0 64 64) (64 64 64) (64 0 64)" "material" "A" } }
  solid { "id" "3" side { "id" "2" "plane" "(0 0 0) (64 0 0) (64 64 0)" "material" "A" } }
}
entity { "id" "5" "classname" "light" "origin" "0 0 0" }
entity { "id" "6" "classname" "light" "origin" "9 9 9" }"#;
    let mut map = hp_vmf::parse(text).expect("parse");
    map.world.solids[1].hidden = true;
    map.entities[1].hidden = true;
    map.visgroups.push(Default::default());
    strip_hidden(&mut map);
    let ids: Vec<u32> = map.world.solids.iter().map(|s| s.id).collect();
    assert_eq!(ids, [2]);
    assert_eq!(map.entities.iter().map(|e| e.id).collect::<Vec<_>>(), [5]);
    assert!(map.visgroups.is_empty());
}

#[test]
fn coarser_lightmaps() {
    let text = r#"world { "id" "1" "classname" "worldspawn"
  solid { "id" "2" side { "id" "1" "plane" "(0 64 64) (64 64 64) (64 0 64)" "material" "A" "lightmapscale" "16" } side { "id" "2" "plane" "(0 0 0) (64 0 0) (64 64 0)" "material" "A" "lightmapscale" "0" } }
}"#;
    let mut map = hp_vmf::parse(text).expect("parse");
    coarsen(&mut map, 1.5);
    let scales: Vec<i32> = map.world.solids[0].sides.iter().map(|s| s.lightmap_scale).collect();
    assert_eq!(scales, [24, 24]);
}

#[test]
fn lighting_entities_and_their_targets_are_the_point_entities_of_a_relight() {
    let text = r#"world { "id" "1" "classname" "worldspawn" }
entity { "id" "5" "classname" "light_spot" "origin" "0 0 0" "target" "Aim" }
entity { "id" "6" "classname" "info_target" "targetname" "aim" "origin" "9 9 9" }
entity { "id" "7" "classname" "info_player_start" "origin" "1 1 1" }
entity { "id" "8" "classname" "light_environment" "origin" "1 1 1" }
entity { "id" "9" "classname" "prop_static" "model" "models/a.mdl" "origin" "1 1 1" }"#;
    let map = hp_vmf::parse(text).expect("parse");
    let ids: Vec<u32> = point_entities(&map).iter().map(|e| e.id).collect();
    assert_eq!(ids, [5, 6, 8]);
}

#[test]
fn moving_a_spot_target_relights_from_the_kept_entities() {
    let spot = "entity { \"id\" \"40\" \"classname\" \"light_spot\" \"origin\" \"128 128 220\" \"_light\" \"255 255 255 900\" \"_cone\" \"30\" \"_inner_cone\" \"20\" \"target\" \"aim\" }\n";
    let aim = |origin: &str| {
        format!(
            "entity {{ \"id\" \"41\" \"classname\" \"info_target\" \"targetname\" \"aim\" \"origin\" \"{origin}\" }}\n"
        )
    };
    let (text, solids) = room_text(&(spot.to_owned() + &aim("40 40 0")));
    let map = hp_vmf::parse(&text).expect("parse");
    let mut h = Harness::new();
    let first = h.baked(job(1, Some(map.clone()), solids.clone()));
    let before = texels_by_bsp(&first);
    let (moved_text, _) = room_text(&(spot.to_owned() + &aim("220 220 0")));
    let moved = hp_vmf::parse(&moved_text).expect("parse");
    let mut edit = job(2, None, HashMap::new());
    edit.points = Some(point_entities(&moved));
    edit.shown = Some(first.layout);
    let patched = h.baked(edit);
    assert!(patched.patch && patched.stats.changed_lights >= 1, "the aimed spot is relit without compiling");
    let after = texels_by_bsp(&patched);
    assert!(after.iter().any(|(k, v)| before.get(k) != Some(v)), "the spot points somewhere else");
    let reference = texels_by_bsp(&Harness::new().baked(job(3, Some(moved), solids)));
    let mut shown = before;
    shown.extend(after);
    let (mut diff, mut peak) = (0.0f32, 0.0f32);
    for (k, v) in &reference {
        for (a, b) in v.iter().zip(&shown[k]) {
            diff = diff.max((lum(*a) - lum(*b)).abs());
            peak = peak.max(lum(*a));
        }
    }
    assert!(diff <= peak * 0.03 + 0.01, "the relit spot matches a fresh bake: {diff} of {peak}");
}

#[test]
fn priorities_round_trip_and_progress_wakes_are_spaced() {
    for p in [BakePriority::Idle, BakePriority::BelowNormal, BakePriority::Normal] {
        assert_eq!(priority_of(priority_code(p)), p);
    }
    assert_eq!(priority_of(9), BakePriority::BelowNormal);
    assert!(wake_due(0, 1));
    assert!(!wake_due(40, 90));
    assert!(wake_due(40, 140));
}

#[test]
fn bakes_stop_while_the_window_is_away() {
    assert!(!away((true, false), None), "no window known: never away");
    assert!(away((true, false), Some((true, true))));
    assert!(!away((true, false), Some((false, false))), "another active window is fine by default");
    assert!(away((false, true), Some((false, false))));
    assert!(!away((false, false), Some((true, false))));
}
