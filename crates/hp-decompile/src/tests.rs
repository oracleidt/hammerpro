use super::*;
use crate::lumps::idx;

#[derive(Default)]
struct BspBuilder {
    planes: Vec<u8>,
    texinfo: Vec<u8>,
    texdata: Vec<u8>,
    strings: Vec<u8>,
    table: Vec<u8>,
    brushes: Vec<u8>,
    sides: Vec<u8>,
    leafbrushes: Vec<u8>,
    leafs: Vec<u8>,
    nodes: Vec<u8>,
    models: Vec<u8>,
    entities: String,
    plane_count: u16,
    side_count: i32,
}

fn f(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn i(out: &mut Vec<u8>, v: i32) {
    out.extend_from_slice(&v.to_le_bytes());
}

impl BspBuilder {
    fn texture(&mut self, name: &str) -> i16 {
        let id = (self.table.len() / 4) as i32;
        i(&mut self.table, self.strings.len() as i32);
        self.strings.extend_from_slice(name.as_bytes());
        self.strings.push(0);
        let td = (self.texdata.len() / 32) as i32;
        let mut rec = vec![0u8; 32];
        rec[12..16].copy_from_slice(&id.to_le_bytes());
        self.texdata.extend_from_slice(&rec);
        let ti = (self.texinfo.len() / 72) as i16;
        for v in [[4.0, 0.0, 0.0, 0.0], [0.0, -4.0, 0.0, 0.0], [0.0625, 0.0, 0.0, 0.0], [0.0, 0.0625, 0.0, 0.0]] {
            for c in v {
                f(&mut self.texinfo, c);
            }
        }
        i(&mut self.texinfo, 0);
        i(&mut self.texinfo, td);
        ti
    }

    fn plane(&mut self, n: [f32; 3], d: f32) -> u16 {
        for c in n {
            f(&mut self.planes, c);
        }
        f(&mut self.planes, d);
        i(&mut self.planes, 0);
        self.plane_count += 1;
        self.plane_count - 1
    }

    fn cube(&mut self, lo: [f32; 3], hi: [f32; 3], contents: i32, tex: i16, bevel_extra: bool) -> u16 {
        let first = self.side_count;
        let mut planes = vec![
            self.plane([1.0, 0.0, 0.0], hi[0]),
            self.plane([-1.0, 0.0, 0.0], -lo[0]),
            self.plane([0.0, 1.0, 0.0], hi[1]),
            self.plane([0.0, -1.0, 0.0], -lo[1]),
            self.plane([0.0, 0.0, 1.0], hi[2]),
            self.plane([0.0, 0.0, -1.0], -lo[2]),
        ];
        if bevel_extra {
            planes.push(self.plane([std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2, 0.0], 0.0));
        }
        let n = planes.len();
        for (k, p) in planes.into_iter().enumerate() {
            self.sides.extend_from_slice(&p.to_le_bytes());
            self.sides.extend_from_slice(&tex.to_le_bytes());
            self.sides.extend_from_slice(&0i16.to_le_bytes());
            self.sides.push(u8::from(bevel_extra && k == n - 1));
            self.sides.push(0);
        }
        self.side_count += n as i32;
        let b = (self.brushes.len() / 12) as u16;
        i(&mut self.brushes, first);
        i(&mut self.brushes, n as i32);
        i(&mut self.brushes, contents);
        b
    }

    fn model(&mut self, brushes: &[u16]) {
        let leaf = (self.leafs.len() / 32) as i32;
        let first = (self.leafbrushes.len() / 2) as u16;
        for b in brushes {
            self.leafbrushes.extend_from_slice(&b.to_le_bytes());
        }
        let mut rec = vec![0u8; 32];
        rec[24..26].copy_from_slice(&first.to_le_bytes());
        rec[26..28].copy_from_slice(&(brushes.len() as u16).to_le_bytes());
        self.leafs.extend_from_slice(&rec);
        self.leafs.extend_from_slice(&[0u8; 32]);
        let node = (self.nodes.len() / 32) as i32;
        let mut n = vec![0u8; 32];
        n[4..8].copy_from_slice(&(-1 - leaf).to_le_bytes());
        n[8..12].copy_from_slice(&(-2 - leaf).to_le_bytes());
        self.nodes.extend_from_slice(&n);
        let mut m = vec![0u8; 48];
        m[36..40].copy_from_slice(&node.to_le_bytes());
        self.models.extend_from_slice(&m);
    }

    fn build(self) -> Vec<u8> {
        let mut ents = self.entities.into_bytes();
        ents.push(0);
        hp_bsp::synthetic_bsp(
            20,
            &[
                (hp_bsp::lump::ENTITIES, ents),
                (idx::PLANES, self.planes),
                (hp_bsp::lump::TEXDATA, self.texdata),
                (idx::NODES, self.nodes),
                (idx::TEXINFO, self.texinfo),
                (hp_bsp::lump::LEAFS, self.leafs),
                (hp_bsp::lump::MODELS, self.models),
                (idx::LEAFBRUSHES, self.leafbrushes),
                (idx::BRUSHES, self.brushes),
                (idx::BRUSHSIDES, self.sides),
                (hp_bsp::lump::TEXDATA_STRING_DATA, self.strings),
                (hp_bsp::lump::TEXDATA_STRING_TABLE, self.table),
            ],
        )
    }
}

fn sample_map() -> Vec<u8> {
    let mut b = BspBuilder::default();
    let dev = b.texture("DEV/DEV_MEASUREGENERIC01");
    let patched = b.texture("maps/gm_sample/concrete/floor_-64_32_0");
    let floor = b.cube([-256.0, -256.0, -16.0], [256.0, 256.0, 0.0], 1, patched, true);
    let detail1 = b.cube([0.0, 0.0, 0.0], [32.0, 32.0, 32.0], 1 | contents::DETAIL, dev, false);
    let detail2 = b.cube([32.0, 0.0, 0.0], [64.0, 32.0, 32.0], 1 | contents::DETAIL, dev, false);
    let far = b.cube([200.0, 200.0, 0.0], [216.0, 216.0, 16.0], 1 | contents::DETAIL, dev, false);
    let door = b.cube([-8.0, -32.0, -64.0], [8.0, 32.0, 64.0], 1, dev, false);
    b.model(&[floor, detail1, detail2, far]);
    b.model(&[door]);
    b.entities = concat!(
        "{\n\"world_maxs\" \"256 256 0\"\n\"skyname\" \"sky_day01_01\"\n\"classname\" \"worldspawn\"\n\"mapversion\" \"12\"\n}\n",
        "{\n\"origin\" \"100 0 64\"\n\"targetname\" \"door1\"\n\"model\" \"*1\"\n\"classname\" \"func_door\"\n\"hammerid\" \"77\"\n}\n",
        "{\n\"origin\" \"0 0 96\"\n\"classname\" \"logic_auto\"\n\"OnMapSpawn\" \"door1,Open,,2,-1\"\n\"hammerid\" \"78\"\n}\n",
    )
    .to_owned();
    b.build()
}

#[test]
fn a_small_map_comes_back() {
    let bsp = Bsp::from_vec(sample_map()).expect("bsp");
    let out = decompile(&bsp, &Options { map_name: "gm_sample".to_owned(), ..Options::default() }).expect("decompiles");
    let r = &out.report;
    assert_eq!(r.brushes, 5);
    assert_eq!(r.failed_brushes, 0, "{r:?}");
    assert_eq!(r.world_solids, 1);
    assert_eq!(r.detail_solids, 3);
    assert_eq!(r.detail_entities, 2, "touching detail brushes share a func_detail");
    assert_eq!(r.entity_solids, 1);
    assert_eq!(r.renamed_materials, 6, "every side of the floor had a patched name");
    let map = &out.map;
    assert_eq!(map.world.get("skyname"), Some("sky_day01_01"));
    assert!(map.world.get("world_maxs").is_none());
    assert!(map.world.get("mapversion").is_none());
    let floor = &map.world.solids[0];
    assert_eq!(floor.sides.len(), 6);
    assert!(floor.sides.iter().all(|s| s.material == "concrete/floor"));
    let g = hp_geom::validate_solid(&floor.sides).expect("valid");
    assert_eq!(g.aabb.min, DVec3::new(-256.0, -256.0, -16.0));
    assert_eq!(g.aabb.max, DVec3::new(256.0, 256.0, 0.0));
    let door = map.entities.iter().find(|e| e.classname == "func_door").expect("door");
    assert_eq!(door.id, 77);
    assert!(door.get("model").is_none());
    let g = hp_geom::validate_solid(&door.solids[0].sides).expect("valid");
    assert_eq!(g.aabb.min, DVec3::new(92.0, -32.0, 0.0));
    assert_eq!(g.aabb.max, DVec3::new(108.0, 32.0, 128.0));
    let top = door.solids[0].sides.iter().find(|s| s.normal().z > 0.9).expect("top");
    assert_eq!(top.uaxis.shift, -400.0, "u = (x - 100) * 4");
    let auto = map.entities.iter().find(|e| e.classname == "logic_auto").expect("auto");
    assert_eq!(auto.id, 78);
    assert_eq!(auto.connections.len(), 1);
    assert_eq!(auto.connections[0].target, "door1");
    assert_eq!(auto.connections[0].delay, 2.0);
    let mut ids = HashSet::new();
    for e in &map.entities {
        assert!(ids.insert(("e", e.id)), "entity id {} twice", e.id);
        for s in &e.solids {
            assert!(ids.insert(("s", s.id)));
        }
    }
    let text = hp_vmf::write(map);
    let back = hp_vmf::parse(&text).expect("reads back");
    assert_eq!(back.entities.len(), map.entities.len());
    assert_eq!(back.world.solids.len(), 1);
}

#[test]
fn toggles_leave_parts_out() {
    let bsp = Bsp::from_vec(sample_map()).expect("bsp");
    let run = |t: Toggles| {
        decompile(&bsp, &Options { map_name: "gm_sample".to_owned(), toggles: t, ..Options::default() })
            .expect("decompiles")
    };
    let all = run(Toggles::default());
    assert_eq!(all.report.world_solids, 1);
    let no_brushes = run(Toggles { brushes: false, ..Toggles::default() });
    assert_eq!(no_brushes.report.world_solids, 0);
    assert_eq!(no_brushes.report.detail_solids, 0);
    assert_eq!(no_brushes.report.failed_brushes, 0);
    assert_eq!(no_brushes.report.entity_solids, 1, "brush entities are a separate switch");
    let no_brush_entities = run(Toggles { brush_entities: false, ..Toggles::default() });
    assert_eq!(no_brush_entities.report.entity_solids, 0);
    assert_eq!(no_brush_entities.report.world_solids, 1);
    let no_entities = run(Toggles { entities: false, ..Toggles::default() });
    assert!(no_entities.map.entities.iter().all(|e| e.classname == "func_detail"));
    assert_eq!(no_entities.report.world_solids, 1);
    let flat = run(Toggles { group_details: false, ..Toggles::default() });
    assert_eq!(flat.report.detail_entities, 0);
    assert_eq!(flat.map.world.solids.len(), 4);
}

#[test]
fn entities_alone_make_a_map() {
    let text = "{
\"classname\" \"worldspawn\"
\"skyname\" \"sky_day01_01\"
\"mapversion\" \"3\"
}
{
\"classname\" \"logic_relay\"
\"targetname\" \"r\"
\"OnTrigger\" \"x,Kill,,0,-1\"
\"hammerid\" \"40\"
}
{
\"classname\" \"func_door\"
\"model\" \"*5\"
}
";
    let out = entities_only(text, &Options::default());
    assert_eq!(out.map.world.get("skyname"), Some("sky_day01_01"));
    assert!(out.map.world.get("mapversion").is_none());
    assert_eq!(out.map.entities.len(), 2);
    assert_eq!(out.map.entities[0].id, 40);
    assert_eq!(out.map.entities[0].connections.len(), 1);
    assert_eq!(out.report.empty_brush_entities, 1);
    assert!(!out.report.warnings.is_empty());
    let none = entities_only(
        text,
        &Options { toggles: Toggles { entities: false, ..Toggles::default() }, ..Options::default() },
    );
    assert!(none.map.entities.is_empty());
}

#[test]
fn hostile_maps_do_not_panic() {
    let full = sample_map();
    let opts = Options::default();
    let mut seed = 0x9E37_79B9u32;
    for round in 0..300 {
        let mut data = full.clone();
        let flips = if round < 100 { 1 } else { 16 };
        for _ in 0..flips {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let at = hp_bsp::HEADER_LEN + (seed as usize) % (data.len() - hp_bsp::HEADER_LEN);
            data[at] = (seed >> 24) as u8;
        }
        if let Ok(bsp) = Bsp::from_vec(data) {
            let _ = decompile(&bsp, &opts);
        }
    }
    let empty = hp_bsp::synthetic_bsp(20, &[]);
    assert!(matches!(decompile(&Bsp::from_vec(empty).expect("bsp"), &opts), Err(DecompileError::Empty)));
}

#[test]
fn real_maps() {
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for dir in [
        "D:/SteamLibrary/steamapps/common/GarrysMod/garrysmod/maps",
        "C:/Program Files (x86)/Steam/steamapps/common/GarrysMod/garrysmod/maps",
    ] {
        for name in ["gm_construct.bsp", "gm_flatgrass.bsp"] {
            let p = std::path::Path::new(dir).join(name);
            if p.is_file() {
                files.push(p);
            }
        }
    }
    if let Ok(extra) = std::env::var("HP_DECOMPILE_EXTRA") {
        files.extend(extra.split(';').filter(|s| !s.is_empty()).map(std::path::PathBuf::from));
    }
    let local =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/e2e/removed_from_game/untitled.bsp");
    if local.is_file() {
        files.push(local);
    }
    for path in files {
        let start = std::time::Instant::now();
        let out = decompile_file(&path, &Options::default()).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let r = &out.report;
        eprintln!("{} in {:?}: {r:#?}", path.display(), start.elapsed());
        assert!(r.world_solids > 0);
        assert!(r.failed_brushes * 50 <= r.brushes - r.empty_brushes, "{}: too many brushes failed", path.display());
        let text = hp_vmf::write(&out.map);
        let back = hp_vmf::parse(&text).expect("reads back");
        let solids: usize = back.all_solids().count();
        assert_eq!(solids, out.map.all_solids().count());
        if let Ok(dir) = std::env::var("HP_DECOMPILE_OUT") {
            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let _ = std::fs::write(std::path::Path::new(&dir).join(format!("{name}_decompiled.vmf")), text);
        }
    }
}

#[test]
fn a_brush_model_that_no_entity_uses_becomes_a_brush_entity_and_not_world_geometry() {
    let mut b = BspBuilder::default();
    let dev = b.texture("DEV/DEV_MEASUREGENERIC01");
    let trig = b.texture("TOOLS/TOOLSTRIGGER");
    let floor = b.cube([-256.0, -256.0, -16.0], [256.0, 256.0, 0.0], 1, dev, false);
    let zone = b.cube([-8.0, -8.0, 0.0], [8.0, 8.0, 64.0], 1, trig, false);
    let wall = b.cube([100.0, -8.0, 0.0], [116.0, 8.0, 64.0], 1, dev, false);
    b.model(&[floor]);
    b.model(&[zone]);
    b.model(&[wall]);
    b.entities = "{\n\"classname\" \"worldspawn\"\n}\n".to_owned();
    let bsp = Bsp::from_vec(b.build()).expect("bsp");
    let out = decompile(&bsp, &Options { map_name: "m".to_owned(), ..Options::default() }).expect("decompiles");
    assert_eq!(out.report.orphan_models, 2);
    assert_eq!(out.report.world_solids, 1);
    let classes: Vec<&str> = out.map.entities.iter().map(|e| e.classname.as_str()).collect();
    assert!(classes.contains(&"trigger_multiple"), "{classes:?}");
    assert!(classes.contains(&"func_brush"), "{classes:?}");
    assert_eq!(out.map.entities.iter().map(|e| e.solids.len()).sum::<usize>(), 2);
}
