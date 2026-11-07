//! Reuse unlit geometry only when edits cannot affect BSP construction or flood filling.
//! Fingerprinting runs on the bake worker, never on the editor thread.

use std::fmt::{Debug, Write};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::{Compiled, Ctx, Outcome};

const BUDGET: usize = 128 << 20;

struct Fingerprint(DefaultHasher);

impl Write for Fingerprint {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        self.0.write(text.as_bytes());
        Ok(())
    }
}

fn hash_debug(hash: &mut Fingerprint, value: &impl Debug) {
    write!(hash, "{value:?}").expect("geometry fingerprint formatting");
}

fn photometric(key: &str) -> bool {
    // Origin is deliberately excluded: point entities seed the compiler's flood fill.
    // Unknown keys, connections and all non-light entities must invalidate geometry.
    [
        "_light",
        "_lighthdr",
        "_lightscalehdr",
        "_ambient",
        "_ambienthdr",
        "_ambientscalehdr",
        "_constant_attn",
        "_linear_attn",
        "_quadratic_attn",
        "_fifty_percent_distance",
        "_zero_percent_distance",
        "_inner_cone",
        "_cone",
        "_exponent",
        "pitch",
        "angles",
    ]
    .iter()
    .any(|known| key.eq_ignore_ascii_case(known))
}

fn key(map: &hp_vmf::Map, factor: f64, fs: &hp_fs::GameFs, resources: u64, hdr: bool) -> u64 {
    let mut hash = Fingerprint(DefaultHasher::new());
    hash_debug(&mut hash, &(&map.version_info, &map.world, &map.cordons, &map.extra_blocks));
    map.entities.len().hash(&mut hash.0);
    for entity in &map.entities {
        let light = entity.solids.is_empty()
            && ["light", "light_spot", "light_environment"]
                .iter()
                .any(|name| entity.classname.eq_ignore_ascii_case(name));
        if !light {
            hash_debug(&mut hash, entity);
            continue;
        }
        hash_debug(
            &mut hash,
            &(
                entity.id,
                &entity.classname,
                &entity.connections,
                &entity.groups,
                &entity.editor,
                entity.hidden,
                &entity.extra_blocks,
            ),
        );
        for property in entity.props.iter().filter(|(key, _)| !photometric(key)) {
            hash_debug(&mut hash, property);
        }
    }
    hash_debug(&mut hash, &(factor.to_bits(), hdr, resources, fs.root(), fs.game_dir(), fs.generation()));
    for source in fs.sources() {
        hash_debug(&mut hash, &source.mount());
    }
    hash.0.finish()
}

pub(super) fn fits_budget(compiled: &Compiled) -> bool {
    compiled.bsp.bytes().len().saturating_add(compiled.sides.estimated_bytes()) <= BUDGET
}

fn refresh(mut compiled: Compiled, map: &hp_vmf::Map, fs: &hp_fs::GameFs, serial: u64) -> Result<Compiled, String> {
    let started = Instant::now();
    let materials = crate::bspc_cli::GameMaterials { fs: fs.clone(), opts: Default::default() };
    let bytes = hp_bspc::compile::entities_only(map, &materials, compiled.bsp.bytes()).map_err(|e| e.to_string())?;
    compiled.bsp = Arc::new(hp_bsp::Bsp::from_vec(bytes).map_err(|e| e.to_string())?);
    compiled.serial = serial;
    compiled.peak = None;
    log::info!(
        "lighting preview: unchanged geometry reused, light entities refreshed in {:.3} s",
        started.elapsed().as_secs_f64()
    );
    Ok(compiled)
}

pub(super) struct Inputs<'a> {
    pub factor: f64,
    pub resources: u64,
    pub hdr: bool,
    pub threads: usize,
    pub serial: u64,
    pub exe: Option<&'a Path>,
}

pub(super) fn prepare(
    cache: &Mutex<Option<Compiled>>,
    map: Option<hp_vmf::Map>,
    fs: &hp_fs::GameFs,
    ctx: &Ctx<'_>,
    inputs: Inputs<'_>,
) -> Result<Compiled, Outcome> {
    let Inputs { factor, resources, hdr, threads, serial, exe } = inputs;
    if ctx.cancelled() {
        return Err(Outcome::Cancelled);
    }
    let cached = cache.lock().ok().and_then(|mut cache| cache.take());
    let Some(map) = map else {
        return cached.filter(|compiled| compiled.serial == serial).ok_or(Outcome::Cancelled);
    };
    let key = key(&map, factor, fs, resources, hdr);
    if let Some(compiled) = cached.filter(|compiled| compiled.geometry_key == Some(key)) {
        match refresh(compiled, &map, fs, serial) {
            Ok(compiled) if !ctx.cancelled() => return Ok(compiled),
            Ok(_) => return Err(Outcome::Cancelled),
            Err(error) => log::debug!("lighting preview: geometry reuse unavailable, compiling again: {error}"),
        }
    }
    if ctx.cancelled() {
        return Err(Outcome::Cancelled);
    }
    let mut compiled = super::compile(map, factor, fs, threads, ctx, serial, exe)?;
    compiled.geometry_key = Some(key);
    Ok(compiled)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::super::{Job, child};

    fn inputs(serial: u64) -> Inputs<'static> {
        Inputs { factor: 1.0, resources: 0, hdr: true, threads: 1, serial, exe: None }
    }
    use super::*;

    fn room() -> hp_vmf::Map {
        let walls = [
            ([-16.0, -16.0, -16.0], [272.0, 272.0, 0.0]),
            ([-16.0, -16.0, 256.0], [272.0, 272.0, 272.0]),
            ([-16.0, -16.0, 0.0], [0.0, 272.0, 256.0]),
            ([256.0, -16.0, 0.0], [272.0, 272.0, 256.0]),
            ([0.0, -16.0, 0.0], [256.0, 0.0, 256.0]),
            ([0.0, 256.0, 0.0], [256.0, 272.0, 256.0]),
        ];
        let mut text = String::from("world { \"id\" \"1\" \"classname\" \"worldspawn\" ");
        for (index, (lo, hi)) in walls.into_iter().enumerate() {
            text += &super::super::tests::solid(2 + index as u32, 10 + index as u32 * 6, lo, hi);
        }
        text += "} entity { \"id\" \"100\" \"classname\" \"light\" \"origin\" \"128 128 128\" \"_light\" \"255 255 255 300\" }
            entity { \"id\" \"101\" \"classname\" \"info_player_start\" \"origin\" \"128 128 64\" }";
        hp_vmf::parse(&text).expect("room")
    }

    #[test]
    fn only_known_light_parameters_can_reuse_geometry() {
        let fs = hp_fs::GameFs::empty();
        for classname in ["light", "light_spot", "light_environment"] {
            let mut map = room();
            map.entities[0].classname = classname.to_owned();
            let baseline = key(&map, 1.0, &fs, 5, true);
            for property in [
                "_LIGHT",
                "_lightHDR",
                "_lightscaleHDR",
                "_ambient",
                "_ambientHDR",
                "_ambientscaleHDR",
                "_constant_attn",
                "_linear_attn",
                "_quadratic_attn",
                "_fifty_percent_distance",
                "_zero_percent_distance",
                "_inner_cone",
                "_cone",
                "_exponent",
                "pitch",
                "angles",
            ] {
                let mut edited = map.clone();
                edited.entities[0].set(property, "42");
                assert_eq!(key(&edited, 1.0, &fs, 5, true), baseline, "{classname}: {property}");
            }
            for property in ["origin", "targetname", "spawnflags", "style", "unknown_compile_key"] {
                let mut edited = map.clone();
                edited.entities[0].set(property, "42");
                assert_ne!(key(&edited, 1.0, &fs, 5, true), baseline, "{classname}: {property}");
            }
            let mut edited = map.clone();
            edited.entities[0].solids.push(edited.world.solids[0].clone());
            let brush_light = key(&edited, 1.0, &fs, 5, true);
            edited.entities[0].set("_light", "10 20 30 40");
            assert_ne!(key(&edited, 1.0, &fs, 5, true), brush_light);
            assert_ne!(key(&map, 2.0, &fs, 5, true), baseline, "coarsening");
            assert_ne!(key(&map, 1.0, &fs, 6, true), baseline, "material/model revision");
            assert_ne!(key(&map, 1.0, &fs, 5, false), baseline, "dynamic range");
        }
        let map = room();
        let baseline = key(&map, 1.0, &fs, 0, true);
        for change in 0..7 {
            let mut edited = map.clone();
            match change {
                0 => edited.world.solids[0].sides[0].plane[0].x += 1.0,
                1 => edited.world.solids[0].sides[0].material = "tools/toolsnodraw".into(),
                2 => edited.world.solids[0].hidden = true,
                3 => edited.entities[1].set("_light", "255 0 0 500"),
                4 => edited.entities[0].hidden = true,
                5 => edited.world.solids[0].sides[0].lightmap_scale = 8,
                6 => {
                    edited.world.solids[0].sides[0].disp =
                        Some(Box::new(hp_vmf::DispInfo::new_flat(2, glam::DVec3::ZERO, glam::DVec3::Z)))
                }
                _ => unreachable!(),
            }
            assert_ne!(key(&edited, 1.0, &fs, 0, true), baseline, "geometry change {change}");
        }
        let dir = std::env::temp_dir().join(format!("hp-light-geometry-key-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        fs.add_game_dir(&dir).unwrap();
        assert_ne!(key(&map, 1.0, &fs, 0, true), baseline, "mounted resources");
        std::fs::remove_dir(&dir).unwrap();
    }

    fn expect_compiled(result: Result<Compiled, Outcome>) -> Compiled {
        match result {
            Ok(compiled) => compiled,
            Err(Outcome::Failed(error)) => panic!("compile: {error}"),
            Err(_) => panic!("compile cancelled"),
        }
    }

    fn assert_same_map(a: &hp_bsp::Bsp, b: &hp_bsp::Bsp) {
        assert_eq!(a.version(), b.version());
        assert_eq!(a.map_revision(), b.map_revision());
        for lump in 0..64 {
            assert_eq!(a.lump_info(lump).unwrap().version, b.lump_info(lump).unwrap().version);
            if lump == hp_bsp::lump::GAME_LUMP {
                // The container may lay out lumps at different file offsets. Game-lump
                // directory offsets are absolute; compare their complete parsed payloads.
                assert_eq!(a.game_lumps().unwrap(), b.game_lumps().unwrap());
            } else {
                assert_eq!(a.lump(lump).unwrap(), b.lump(lump).unwrap(), "lump {lump}");
            }
        }
    }

    #[test]
    fn refreshed_light_entities_preserve_all_geometry_lumps() {
        let fs = hp_fs::GameFs::empty();
        let map = room();
        let wanted = AtomicU64::new(1);
        let (out, _) = crossbeam_channel::unbounded();
        let ctx = Ctx { id: 1, wanted: &wanted, out: &out, wake: &|| {}, start: Instant::now(), faces: 0, game: None };
        let cache = Mutex::default();
        let original = expect_compiled(prepare(&cache, Some(map.clone()), &fs, &ctx, inputs(1)));
        assert!(fits_budget(&original));
        super::super::retain_compiled(&cache, &ctx, &original, false);
        let mut edited = map.clone();
        edited.entities[0].set("_light", "20 40 255 600");
        let changed = expect_compiled(prepare(&cache, Some(edited.clone()), &fs, &ctx, inputs(2)));
        assert!(Arc::ptr_eq(&changed.sides, &original.sides), "the existing side bindings were used");
        assert_eq!(changed.serial, 2);
        assert_ne!(changed.bsp.lump(0).unwrap(), original.bsp.lump(0).unwrap());
        for lump in 1..64 {
            assert_eq!(changed.bsp.lump(lump).unwrap(), original.bsp.lump(lump).unwrap(), "geometry lump {lump}");
        }
        assert!(
            changed
                .bsp
                .entities()
                .unwrap()
                .iter()
                .any(|e| e.classname() == "light" && e.get("_light") == Some("20 40 255 600"))
        );
        let fresh = expect_compiled(prepare(&Mutex::default(), Some(edited.clone()), &fs, &ctx, inputs(2)));
        assert_same_map(&changed.bsp, &fresh.bsp);

        // Moving a light can change leak detection and outside-face removal. It must compile again.
        super::super::retain_compiled(&cache, &ctx, &changed, false);
        edited.entities[0].set("origin", "144 128 128");
        let moved = expect_compiled(prepare(&cache, Some(edited), &fs, &ctx, inputs(3)));
        assert!(!Arc::ptr_eq(&moved.sides, &changed.sides));

        wanted.store(0, Ordering::Release);
        assert!(matches!(prepare(&cache, Some(map), &fs, &ctx, inputs(4)), Err(Outcome::Cancelled)));
    }

    #[test]
    fn cached_light_edit_matches_fresh_lightmaps_and_changes_the_result() {
        let wanted = AtomicU64::new(1);
        let (out, _) = crossbeam_channel::unbounded();
        let ctx = Ctx { id: 1, wanted: &wanted, out: &out, wake: &|| {}, start: Instant::now(), faces: 0, game: None };
        let map = room();
        let make_job = |map, serial| Job {
            id: 1,
            serial,
            map: Some(map),
            keep: false,
            solids: Default::default(),
            fs: None,
            resources_revision: 0,
            hdr: true,
            quality: hp_bspc::rad::RadQuality::Draft,
            bounces: 1,
            prop_shadows: true,
            prop_lighting: true,
            threads: (1, 1),
            layer_size: 256,
            max_layers: 4,
            coarsen: 1.0,
            lit: false,
            tuning: child::Tuning::default(),
            exe: None,
        };
        let bake = |job, cache: &Mutex<Option<Compiled>>| match super::super::run(job, &ctx, cache) {
            Outcome::Baked(baked) => baked,
            Outcome::Failed(error) => panic!("bake: {error}"),
            Outcome::Cancelled => panic!("bake cancelled"),
        };
        let cache = Mutex::default();
        let before = bake(make_job(map.clone(), 1), &cache);
        let original = cache.lock().unwrap().clone().unwrap();
        let mut edited = map;
        edited.entities[0].set("_light", "30 255 50 600");
        let reused = bake(make_job(edited.clone(), 2), &cache);
        assert!(Arc::ptr_eq(&cache.lock().unwrap().as_ref().unwrap().sides, &original.sides));
        let fresh = bake(make_job(edited, 2), &Mutex::default());
        assert_ne!(before.texels, reused.texels, "the edited light must change the lightmaps");
        assert_eq!(reused.texels, fresh.texels, "every packed lightmap pixel matches the full bake");
        assert_eq!(reused.faces, fresh.faces);
        assert_eq!(reused.luxels, fresh.luxels);
        assert_eq!(reused.leaked, fresh.leaked);
    }

    #[test]
    fn incompatible_cached_entities_fall_back_to_a_full_compile() {
        let wanted = AtomicU64::new(1);
        let (out, _) = crossbeam_channel::unbounded();
        let ctx = Ctx { id: 1, wanted: &wanted, out: &out, wake: &|| {}, start: Instant::now(), faces: 0, game: None };
        let fs = hp_fs::GameFs::empty();
        let map = room();
        let cache = Mutex::default();
        let mut original = expect_compiled(prepare(&cache, Some(map.clone()), &fs, &ctx, inputs(1)));
        let sides = original.sides.clone();
        let mut entities = original.bsp.entities().unwrap();
        entities.push(hp_bsp::Entity {
            pairs: vec![("classname".into(), "func_smokevolume".into()), ("origin".into(), "1 0 0".into())],
        });
        let mut edit = hp_bsp::Bsp::from_vec(original.bsp.bytes().to_vec()).unwrap().edit();
        edit.set_entities(&entities).unwrap();
        original.bsp = Arc::new(hp_bsp::Bsp::from_vec(edit.to_vec().unwrap()).unwrap());
        *cache.lock().unwrap() = Some(original);
        let compiled = expect_compiled(prepare(&cache, Some(map), &fs, &ctx, inputs(2)));
        assert!(!Arc::ptr_eq(&compiled.sides, &sides), "an incompatible cached BSP must not block the bake");
        assert!(!compiled.bsp.entities().unwrap().iter().any(|e| e.classname() == "func_smokevolume"));
    }

    #[test]
    #[ignore = "manual geometry preparation benchmark; prints timings without timing-dependent assertions"]
    fn benchmark_light_edit_geometry_reuse() {
        let mut map = room();
        for y in 0..12 {
            for x in 0..12 {
                let id = 1000 + x + y * 12;
                let lo = [8.0 + x as f64 * 20.0, 8.0 + y as f64 * 20.0, 0.0];
                let hi = [lo[0] + 8.0, lo[1] + 8.0, 48.0];
                let text = format!("world {{ {} }}", super::super::tests::solid(id, 10_000 + (x + y * 12) * 6, lo, hi));
                map.world.solids.extend(hp_vmf::parse(&text).unwrap().world.solids);
            }
        }
        let fs = hp_fs::GameFs::empty();
        let wanted = AtomicU64::new(1);
        let (out, _) = crossbeam_channel::unbounded();
        let ctx = Ctx { id: 1, wanted: &wanted, out: &out, wake: &|| {}, start: Instant::now(), faces: 0, game: None };
        let cache = Mutex::default();
        let compiled = expect_compiled(prepare(&cache, Some(map.clone()), &fs, &ctx, inputs(1)));
        eprintln!(
            "GEOMETRY_BENCH brushes={} bsp_bytes={} side_bytes={} retained_budget_bytes={BUDGET}",
            map.world.solids.len(),
            compiled.bsp.bytes().len(),
            compiled.sides.estimated_bytes()
        );
        super::super::retain_compiled(&cache, &ctx, &compiled, false);
        for sample in 0..3 {
            map.entities[0].set("_light", format!("20 40 255 {}", 400 + sample * 100));
            let started = Instant::now();
            let reused = expect_compiled(prepare(&cache, Some(map.clone()), &fs, &ctx, inputs(2)));
            let reused_ms = started.elapsed().as_secs_f64() * 1000.0;
            let started = Instant::now();
            let fresh = expect_compiled(prepare(&Mutex::default(), Some(map.clone()), &fs, &ctx, inputs(2)));
            let fresh_ms = started.elapsed().as_secs_f64() * 1000.0;
            assert!(Arc::ptr_eq(&reused.sides, &compiled.sides));
            assert_same_map(&reused.bsp, &fresh.bsp);
            eprintln!(
                "GEOMETRY_BENCH sample={sample} cached_ms={reused_ms:.3} full_ms={fresh_ms:.3} speedup={:.2}",
                fresh_ms / reused_ms
            );
            super::super::retain_compiled(&cache, &ctx, &reused, false);
        }
    }
}
