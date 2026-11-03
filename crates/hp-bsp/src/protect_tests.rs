use std::collections::{BTreeMap, BTreeSet};

use crate::gamelump::id;
use crate::header::lump;
use crate::layout::LayoutOptions;
use crate::reorder::ReorderOptions;
use crate::staticprops::{StaticPropFormat, StaticPropLump, StaticPropRecord};
use crate::*;

const PLANES: usize = 24;
const VERTEXES: usize = 40;
const EDGES: usize = 48;
const SURFEDGES: usize = 96;
const MATERIALS: usize = 7;
const TEXINFOS: usize = 9;
const FACES: usize = 24;
const BRUSHES: usize = 10;
const NODES: usize = 6;
const LEAFS: usize = 7;

const STOCK: [&str; MATERIALS] = [
    "concrete/floor01",
    "brick/wall02",
    "TOOLS/TOOLSNODRAW",
    "custom/mine",
    "glass/window03",
    "maps/gm_sample/concrete/floor01_1_2_3",
    "custom/second",
];

const CUSTOM: [&str; 2] = ["custom/mine", "custom/second"];

const STOCK_RANDOMIZED: usize = 3;

fn push16(v: &mut Vec<u8>, x: i64) {
    v.extend_from_slice(&(x as i16).to_le_bytes());
}

fn push32(v: &mut Vec<u8>, x: i64) {
    v.extend_from_slice(&(x as i32).to_le_bytes());
}

fn pushf(v: &mut Vec<u8>, x: f32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn sample_lumps() -> Vec<(usize, Vec<u8>)> {
    let mut planes = Vec::new();
    for p in 0..PLANES {
        let axis = p % 3;
        let sign = if p % 6 < 3 { 1.0 } else { -1.0 };
        for k in 0..3 {
            pushf(&mut planes, if k == axis { sign } else { 0.0 });
        }
        pushf(&mut planes, 16.0 * (p as f32 + 1.0));
        push32(&mut planes, axis as i64);
    }
    let mut vertexes = Vec::new();
    for v in 0..VERTEXES {
        pushf(&mut vertexes, (v % 8) as f32 * 32.0);
        pushf(&mut vertexes, (v / 8) as f32 * 48.0);
        pushf(&mut vertexes, (v % 5) as f32 * 16.0);
    }
    let mut edges = vec![0u8; 4];
    for e in 1..EDGES {
        push16(&mut edges, (e % VERTEXES) as i64);
        push16(&mut edges, ((e * 3 + 1) % VERTEXES) as i64);
    }
    let mut surfedges = Vec::new();
    for s in 0..SURFEDGES {
        let e = 1 + (s % (EDGES - 1)) as i64;
        push32(&mut surfedges, if s % 3 == 0 { -e } else { e });
    }
    let mut string_data = Vec::new();
    let mut string_table = Vec::new();
    for name in STOCK {
        push32(&mut string_table, string_data.len() as i64);
        string_data.extend_from_slice(name.as_bytes());
        string_data.push(0);
    }
    let mut texdata = Vec::new();
    for m in 0..MATERIALS {
        for _ in 0..3 {
            pushf(&mut texdata, 0.2);
        }
        push32(&mut texdata, m as i64);
        push32(&mut texdata, 256);
        push32(&mut texdata, 256);
        push32(&mut texdata, 256);
        push32(&mut texdata, 256);
    }
    let mut texinfo = Vec::new();
    for t in 0..TEXINFOS {
        for row in 0..4 {
            for k in 0..4 {
                pushf(&mut texinfo, if k == row % 3 { 0.25 } else { 0.0 });
            }
        }
        push32(&mut texinfo, if t == 2 { 0x80 } else { 0 });
        push32(&mut texinfo, (t % MATERIALS) as i64);
    }
    let mut faces = Vec::new();
    let mut original = Vec::new();
    for f in 0..FACES {
        for out in [&mut faces, &mut original] {
            let at = out.len();
            push16(out, (f % PLANES) as i64);
            out.extend_from_slice(&[(f % 2) as u8, 1]);
            push32(out, ((f * 3) % (SURFEDGES - 4)) as i64);
            push16(out, 3);
            push16(out, (f % TEXINFOS) as i64);
            push16(out, -1);
            push16(out, -1);
            out.extend_from_slice(&[0, 255, 255, 255]);
            push32(out, 1000 + f as i64);
            pushf(out, 1.0);
            out.resize(at + 56, 0);
        }
    }
    let mut nodes = Vec::new();
    for n in 0..NODES {
        push32(&mut nodes, (n % PLANES) as i64);
        push32(&mut nodes, -1 - (n as i64));
        push32(&mut nodes, -1 - ((n + 1) as i64));
        nodes.extend(vec![0u8; 12]);
        push16(&mut nodes, ((n * 4) % (FACES - 4)) as i64);
        push16(&mut nodes, 4);
        push16(&mut nodes, 0);
        push16(&mut nodes, 0);
    }
    let mut leafs = Vec::new();
    let mut leaffaces = Vec::new();
    let mut leafbrushes = Vec::new();
    for l in 0..LEAFS {
        push32(&mut leafs, l as i64);
        push16(&mut leafs, l as i64);
        push16(&mut leafs, 0);
        leafs.extend(vec![0u8; 12]);
        push16(&mut leafs, (leaffaces.len() / 2) as i64);
        push16(&mut leafs, 2);
        for k in 0..2 {
            push16(&mut leaffaces, ((l + k) % FACES) as i64);
        }
        push16(&mut leafs, (leafbrushes.len() / 2) as i64);
        push16(&mut leafs, 2);
        for k in 0..2 {
            push16(&mut leafbrushes, ((l * 2 + k) % BRUSHES) as i64);
        }
        push16(&mut leafs, -1);
        push16(&mut leafs, 0);
    }
    let mut brushes = Vec::new();
    let mut sides = Vec::new();
    for b in 0..BRUSHES {
        push32(&mut brushes, (sides.len() / 8) as i64);
        push32(&mut brushes, 6);
        push32(&mut brushes, 1 + b as i64);
        for k in 0..6 {
            push16(&mut sides, ((b + k * 2) % PLANES) as i64);
            push16(&mut sides, ((b + k) % TEXINFOS) as i64);
            push16(&mut sides, -1);
            push16(&mut sides, 0);
        }
    }
    let entities = crate::entities::write_entities(&sample_entities());
    vec![
        (lump::ENTITIES, entities),
        (1, planes),
        (lump::TEXDATA, texdata),
        (3, vertexes),
        (5, nodes),
        (6, texinfo),
        (lump::FACES, faces),
        (lump::ORIGINAL_FACES, original),
        (lump::LEAFS, leafs),
        (12, edges),
        (13, surfedges),
        (16, leaffaces),
        (17, leafbrushes),
        (18, brushes),
        (19, sides),
        (lump::TEXDATA_STRING_DATA, string_data),
        (lump::TEXDATA_STRING_TABLE, string_table),
    ]
}

fn ent(pairs: &[(&str, &str)]) -> Entity {
    Entity { pairs: pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect() }
}

fn sample_entities() -> Vec<Entity> {
    vec![
        ent(&[
            ("classname", "worldspawn"),
            ("skyname", "sky_day01_01"),
            ("mapversion", "7"),
            ("comment", "internal note"),
            ("hammerid", "1"),
        ]),
        ent(&[("classname", "logic_auto"), ("OnMapSpawn", "relay,Trigger,,0,-1"), ("hammerid", "2")]),
        ent(&[("classname", "math_counter"), ("targetname", "counter"), ("hammerid", "3")]),
        ent(&[("classname", "info_node"), ("origin", "16 32 48")]),
        ent(&[
            ("classname", "func_door"),
            ("model", "*1"),
            ("speed", "100"),
            ("OnFullyOpen", "counter,Add,1,0,-1"),
            ("parentname", "rig"),
            ("hammerid", "4"),
        ]),
        ent(&[("classname", "trigger_multiple"), ("model", "*2"), ("OnStartTouch", "door,Open,,0,-1")]),
        ent(&[("classname", "env_cubemap"), ("origin", "0 0 64"), ("cubemapsize", "0")]),
        ent(&[("classname", "prop_dynamic"), ("model", "models/sample/thing.mdl"), ("targetname", "thing")]),
        ent(&[("classname", "light"), ("origin", "0 0 128"), ("_light", "255 255 255 200")]),
    ]
}

fn props_lump() -> (Vec<u8>, u16) {
    let names = vec!["models/sample/thing.mdl".to_owned(), "models/sample/other.mdl".to_owned()];
    let props: Vec<StaticPropRecord> = (0..5)
        .map(|i| StaticPropRecord {
            origin: [i as f32 * 32.0, 16.0, 8.0],
            angles: [0.0, i as f32 * 15.0, 0.0],
            prop_type: (i % 2) as u16,
            first_leaf: 0,
            leaf_count: 1,
            solid: 6,
            flags: 0x100,
            skin: i,
            fade_min: 500.0,
            fade_max: 1200.0,
            ..StaticPropRecord::default()
        })
        .collect();
    let lump = StaticPropLump { names, leaves: vec![0u16], props };
    (lump.to_bytes(StaticPropFormat::V10), 10)
}

fn vmt_of(name: &str) -> Vec<u8> {
    format!("\"LightmappedGeneric\"\n{{\n\t\"$basetexture\" \"{name}\"\n}}\n").into_bytes()
}

fn sample_bsp() -> Bsp {
    let plain = Bsp::from_vec(crate::synthetic_bsp(20, &sample_lumps())).expect("sample bsp");
    let mut pak = plain.pakfile().expect("pakfile").to_writer().expect("writer");
    for name in CUSTOM {
        pak.add(&format!("materials/{name}.vmt"), vmt_of(name)).expect("vmt");
        pak.add(&format!("materials/{name}.vtf"), vec![7u8; 96]).expect("vtf");
    }
    pak.add("models/sample/thing.mdl", vec![3u8; 64]).expect("mdl");
    let mut edit = Bsp::from_vec(plain.bytes().to_vec()).expect("copy").edit();
    edit.set_pakfile(pak.finish(crate::PakOptions::default()).expect("zip")).expect("set pakfile");
    let (props, version) = props_lump();
    let mut game = GameLumps::default();
    game.set(id::STATIC_PROPS, version, props).expect("game lump");
    edit.set_game_lumps(game);
    Bsp::from_vec(edit.to_vec().expect("bytes")).expect("read back")
}

fn strongest(seed: &str) -> ProtectOptions {
    ProtectOptions {
        watermark: "author42".into(),
        salt: "pepper".into(),
        randomize_materials: true,
        strip_original_faces: true,
        lock: Some(Lock { hash: lock_hash("secret"), block: true }),
        layout: LayoutOptions::strongest(),
        reorder: ReorderOptions { tables: true, world: true },
        layout_seed: seed.into(),
        ..Default::default()
    }
}

fn texture_names(bsp: &Bsp) -> Vec<String> {
    let data = bsp.lump(lump::TEXDATA_STRING_DATA).expect("string data").into_owned();
    let table = bsp.lump(lump::TEXDATA_STRING_TABLE).expect("string table").into_owned();
    table
        .chunks_exact(4)
        .map(|c| {
            let at = u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize;
            let end = data[at..].iter().position(|b| *b == 0).expect("terminator");
            String::from_utf8_lossy(&data[at..at + end]).into_owned()
        })
        .collect()
}

fn material_of_texinfo(bsp: &Bsp, texinfo: i32) -> String {
    let names = texture_names(bsp);
    let ti = bsp.lump(6).expect("texinfo").into_owned();
    let td = bsp.lump(lump::TEXDATA).expect("texdata").into_owned();
    let at = texinfo as usize * 72;
    let data = i32::from_le_bytes(ti[at + 68..at + 72].try_into().expect("4 bytes")) as usize;
    let id = i32::from_le_bytes(td[data * 32 + 12..data * 32 + 16].try_into().expect("4 bytes")) as usize;
    names[id].clone()
}

fn plane_of(bsp: &Bsp, index: usize) -> String {
    let planes = bsp.lump(1).expect("planes").into_owned();
    let at = index * 20;
    let f = |k: usize| f32::from_le_bytes(planes[at + k * 4..at + k * 4 + 4].try_into().expect("4 bytes"));
    format!("{:.3},{:.3},{:.3},{:.3}", f(0), f(1), f(2), f(3))
}

fn resolved_brushes(bsp: &Bsp) -> Vec<(i32, Vec<(String, String)>)> {
    let brushes = bsp.lump(18).expect("brushes").into_owned();
    let sides = bsp.lump(19).expect("sides").into_owned();
    let mut out = Vec::new();
    for b in brushes.chunks_exact(12) {
        let first = i32::from_le_bytes(b[0..4].try_into().expect("4 bytes")) as usize;
        let count = i32::from_le_bytes(b[4..8].try_into().expect("4 bytes")) as usize;
        let contents = i32::from_le_bytes(b[8..12].try_into().expect("4 bytes"));
        let mut list = Vec::new();
        for k in 0..count {
            let at = (first + k) * 8;
            let plane = u16::from_le_bytes(sides[at..at + 2].try_into().expect("2 bytes")) as usize;
            let texinfo = i16::from_le_bytes(sides[at + 2..at + 4].try_into().expect("2 bytes")) as i32;
            list.push((plane_of(bsp, plane), material_of_texinfo(bsp, texinfo)));
        }
        list.sort();
        out.push((contents, list));
    }
    out
}

fn resolved_faces(bsp: &Bsp) -> BTreeSet<String> {
    let faces = bsp.lump(lump::FACES).expect("faces").into_owned();
    let surfedges = bsp.lump(13).expect("surfedges").into_owned();
    let edges = bsp.lump(12).expect("edges").into_owned();
    let vertexes = bsp.lump(3).expect("vertexes").into_owned();
    let mut out = BTreeSet::new();
    for f in faces.chunks_exact(56) {
        let plane = u16::from_le_bytes(f[0..2].try_into().expect("2 bytes")) as usize;
        let first = i32::from_le_bytes(f[4..8].try_into().expect("4 bytes")) as usize;
        let num = i16::from_le_bytes(f[8..10].try_into().expect("2 bytes")) as usize;
        let texinfo = i16::from_le_bytes(f[10..12].try_into().expect("2 bytes")) as i32;
        let mut points = Vec::new();
        for k in 0..num {
            let se = i32::from_le_bytes(surfedges[(first + k) * 4..(first + k) * 4 + 4].try_into().expect("4 bytes"));
            let e = se.unsigned_abs() as usize;
            let a = u16::from_le_bytes(edges[e * 4..e * 4 + 2].try_into().expect("2 bytes"));
            let b = u16::from_le_bytes(edges[e * 4 + 2..e * 4 + 4].try_into().expect("2 bytes"));
            let v = usize::from(if se >= 0 { a } else { b });
            let c = |k: usize| f32::from_le_bytes(vertexes[v * 12 + k * 4..v * 12 + k * 4 + 4].try_into().expect("4"));
            points.push(format!("{:.2}/{:.2}/{:.2}", c(0), c(1), c(2)));
        }
        out.insert(format!("{} {} {}", plane_of(bsp, plane), material_of_texinfo(bsp, texinfo), points.join(",")));
    }
    out
}

fn resolved_props(bsp: &Bsp) -> Vec<String> {
    let game = bsp.game_lumps().expect("game lumps");
    let Some(g) = game.get(id::STATIC_PROPS) else { return Vec::new() };
    let data = g.data().expect("prop data").into_owned();
    let (lump, _) = StaticPropLump::parse(&data, g.version, None).expect("props");
    lump.props
        .iter()
        .map(|p| {
            format!(
                "{} {:?} {:?} {} {} {}",
                lump.names[usize::from(p.prop_type)],
                p.origin,
                p.angles,
                p.skin,
                p.fade_min,
                p.fade_max
            )
        })
        .collect()
}

fn pak_names(bsp: &Bsp) -> BTreeSet<String> {
    bsp.pakfile().expect("pakfile").entries().iter().map(|e| e.key.clone()).collect()
}

fn patch_targets(bsp: &Bsp) -> BTreeMap<String, String> {
    let pak = bsp.pakfile().expect("pakfile");
    let mut out = BTreeMap::new();
    for e in pak.entries() {
        let Some(name) = e.key.strip_prefix("materials/").and_then(|n| n.strip_suffix(".vmt")) else { continue };
        let Ok(Some(bytes)) = pak.read_name(&e.name) else { continue };
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if !text.to_ascii_lowercase().contains("\"patch\"") {
            continue;
        }
        let include = text
            .lines()
            .find(|l| l.to_ascii_lowercase().contains("include"))
            .and_then(|l| {
                let mut q = l.match_indices('"').map(|(i, _)| i);
                let (_, _, a, b) = (q.next()?, q.next()?, q.next()?, q.next()?);
                Some(l[a + 1..b].to_owned())
            })
            .expect("include line");
        let target = include.trim_start_matches("materials/").trim_end_matches(".vmt").to_ascii_lowercase();
        out.insert(name.to_ascii_lowercase(), target);
    }
    out
}

fn is_garbage(name: &str) -> bool {
    name.strip_prefix("hp/").is_some_and(|r| r.len() == 8 && r.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn classes(bsp: &Bsp) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for e in bsp.entities().expect("entities") {
        *out.entry(e.classname().to_ascii_lowercase()).or_default() += 1;
    }
    out
}

#[test]
fn the_sample_map_is_well_formed() {
    let bsp = sample_bsp();
    assert_eq!(resolved_brushes(&bsp).len(), BRUSHES);
    assert_eq!(resolved_faces(&bsp).len(), FACES);
    assert_eq!(resolved_props(&bsp).len(), 5);
    assert_eq!(texture_names(&bsp), STOCK);
    assert!(pak_names(&bsp).contains("materials/custom/mine.vmt"));
    assert!(!bsp.lump(lump::ORIGINAL_FACES).expect("original faces").is_empty());
}

#[test]
fn the_strongest_protection_keeps_the_resolved_world_and_drops_the_gameplay() {
    let full = sample_bsp();
    let (bytes, report) = protect(&full, &strongest("seed-one")).expect("protect");
    let out = Bsp::from_vec(bytes).expect("the protected map reads back");

    same_world(&full, &out, "strongest");
    assert_eq!(resolved_props(&out), resolved_props(&full));
    assert_eq!(out.lump(lump::VISIBILITY).expect("vis"), full.lump(lump::VISIBILITY).expect("vis"));

    let kept = classes(&out);
    assert!(!kept.contains_key("logic_auto") && !kept.contains_key("math_counter") && !kept.contains_key("info_node"));
    assert_eq!(kept.get("func_door"), Some(&1), "a brush entity keeps its geometry");
    assert_eq!(kept.get("trigger_multiple"), Some(&1));
    assert_eq!(kept.get("env_cubemap"), Some(&1));
    assert_eq!(kept.get("prop_dynamic"), Some(&1));
    assert_eq!(kept.get("light"), Some(&1));
    let ents = out.entities().expect("entities");
    let world = ents.iter().find(|e| e.classname() == "worldspawn").expect("worldspawn");
    assert_eq!(world.get("skyname"), Some("sky_day01_01"));
    assert_eq!(world.get("hammerpro_protected"), Some("author42"));
    assert_eq!(world.get("hammerpro_lock"), Some(lock_hash("secret").as_str()));
    assert_eq!(world.get("hammerpro_lock_block"), Some("1"));
    assert_eq!(world.get("comment"), None);
    assert_eq!(world.get("hammerid"), None);
    let door = ents.iter().find(|e| e.classname() == "func_door").expect("door");
    assert_eq!(door.get("speed"), Some("100"));
    assert_eq!(door.get("model"), Some("*1"));
    assert!(door.get("OnFullyOpen").is_none() && door.get("parentname").is_none());
    assert_eq!((report.entities_removed, report.connections_stripped), (3, 2));
    assert_eq!(report.entities_kept + report.entities_removed, report.entities_before);

    assert!(out.lump(lump::ORIGINAL_FACES).expect("original faces").is_empty());
    assert_eq!(report.original_faces_bytes, FACES * 56);

    let names = texture_names(&out);
    for (old, new) in STOCK.iter().zip(&names) {
        let keep = old.starts_with("TOOLS/") || old.starts_with("maps/");
        if keep {
            assert_eq!(new, old, "the engine must still recognise {old}");
        } else {
            assert!(is_garbage(new), "{old} became {new}");
        }
    }
    let patches = patch_targets(&out);
    for (old, new) in STOCK.iter().zip(&names) {
        if !is_garbage(new) || CUSTOM.contains(old) {
            continue;
        }
        assert_eq!(patches.get(&new.to_ascii_lowercase()), Some(&old.to_ascii_lowercase()), "patch of {old}");
    }
    let pak = pak_names(&out);
    for name in CUSTOM {
        assert!(!pak.contains(&format!("materials/{name}.vmt")), "{name} kept its name");
        assert!(!pak.contains(&format!("materials/{name}.vtf")), "{name} texture kept its name");
    }
    assert!(pak.contains("models/sample/thing.mdl"), "the model files are untouched");
    assert_eq!(report.custom_assets_renamed, CUSTOM.len());
    assert_eq!(report.materials_randomized, STOCK_RANDOMIZED + CUSTOM.len());
    assert_eq!(patches.len(), STOCK_RANDOMIZED, "only the stock materials get a patch material");
    assert!(!report.layout_seed.is_empty() && !report.reordered.is_empty());
    assert!(report.decoy_blocks > 0 && report.gap_bytes > 0);
}

fn rename_map(full: &Bsp, out: &Bsp) -> BTreeMap<String, String> {
    texture_names(full).into_iter().zip(texture_names(out)).collect()
}

fn renamed(map: &BTreeMap<String, String>, name: &str) -> String {
    map.get(name).cloned().unwrap_or_else(|| name.to_owned())
}

fn expected_brushes(full: &Bsp, out: &Bsp) -> Vec<(i32, Vec<(String, String)>)> {
    let map = rename_map(full, out);
    resolved_brushes(full)
        .into_iter()
        .map(|(c, sides)| {
            let mut list: Vec<(String, String)> = sides.into_iter().map(|(p, m)| (p, renamed(&map, &m))).collect();
            list.sort();
            (c, list)
        })
        .collect()
}

fn expected_faces(full: &Bsp, out: &Bsp) -> BTreeSet<String> {
    let map = rename_map(full, out);
    resolved_faces(full)
        .into_iter()
        .map(|line| {
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            format!("{} {} {}", parts[0], renamed(&map, parts[1]), parts[2])
        })
        .collect()
}

fn same_world(full: &Bsp, out: &Bsp, what: &str) {
    let (mut mine, mut want) = (resolved_brushes(out), expected_brushes(full, out));
    mine.sort();
    want.sort();
    assert_eq!(mine, want, "{what}: a brush resolves differently after the protection");
    assert_eq!(resolved_faces(out), expected_faces(full, out), "{what}: a face resolves differently");
}

#[test]
fn every_option_on_its_own_leaves_a_readable_map() {
    let full = sample_bsp();
    let none = LayoutOptions::default();
    let variants: Vec<(&str, LayoutOptions, ReorderOptions)> = vec![
        ("lumps", LayoutOptions { shuffle_lumps: true, ..none.clone() }, ReorderOptions::default()),
        ("gaps", LayoutOptions { junk_gaps: true, ..none.clone() }, ReorderOptions::default()),
        ("decoys", LayoutOptions { decoys: true, ..none.clone() }, ReorderOptions::default()),
        ("header", LayoutOptions { scramble_header: true, ..none.clone() }, ReorderOptions::default()),
        ("gamelumps", LayoutOptions { shuffle_game_lumps: true, ..none.clone() }, ReorderOptions::default()),
        ("pak", LayoutOptions { random_pak: true, ..none.clone() }, ReorderOptions::default()),
        ("tables", none.clone(), ReorderOptions { tables: true, world: false }),
        ("world", none.clone(), ReorderOptions { tables: false, world: true }),
        ("everything", LayoutOptions::strongest(), ReorderOptions { tables: true, world: true }),
    ];
    for (name, layout, reorder) in variants {
        let opts = ProtectOptions { layout, reorder, layout_seed: format!("seed-{name}"), ..strongest("x") };
        let (bytes, _) = protect(&full, &opts).unwrap_or_else(|e| panic!("{name}: {e}"));
        let out = Bsp::from_vec(bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        same_world(&full, &out, name);
        assert_eq!(resolved_props(&out), resolved_props(&full), "{name}: static props");
        let patches = patch_targets(&out);
        assert_eq!(patches.len(), STOCK_RANDOMIZED, "{name}: patch materials");
        assert_eq!(pak_names(&out).len(), pak_names(&full).len() + patches.len(), "{name}: pakfile");
        assert_eq!(out.entities().expect("entities").len(), 6, "{name}: entities");
    }
}

#[test]
fn protecting_twice_with_the_same_seed_gives_the_same_bytes() {
    let full = sample_bsp();
    let (a, ra) = protect(&full, &strongest("same")).expect("first");
    let (b, _) = protect(&full, &strongest("same")).expect("second");
    assert_eq!(a, b, "the same seed must give the same file");
    let (c, rc) = protect(&full, &strongest("other")).expect("third");
    assert_ne!(a, c, "another seed must give another file");
    assert_ne!(ra.layout_seed, rc.layout_seed);
    let out_a = Bsp::from_vec(a).expect("a");
    let out_c = Bsp::from_vec(c).expect("c");
    assert_eq!(resolved_props(&out_a), resolved_props(&out_c));
    same_world(&full, &out_a, "same seed");
    same_world(&full, &out_c, "other seed");
}

#[test]
fn the_server_copy_of_a_shuffled_protected_map_has_every_entity_back() {
    let full = sample_bsp();
    let opts = strongest("server");
    let (client, _) = protect(&full, &opts).expect("protect");
    let server = server_copy(&client, &full, &opts).expect("server copy");
    let client_bsp = Bsp::from_vec(client).expect("client");
    let server_bsp = Bsp::from_vec(server).expect("server");
    let before = classes(&full);
    assert_eq!(classes(&server_bsp), before, "the server keeps every class");
    let world =
        server_bsp.entities().expect("entities").into_iter().find(|e| e.classname() == "worldspawn").expect("world");
    assert_eq!(world.get("hammerpro_protected"), Some("author42"));
    let logic = server_bsp
        .entities()
        .expect("entities")
        .into_iter()
        .find(|e| e.classname() == "logic_auto")
        .expect("logic entity");
    assert_eq!(logic.get("OnMapSpawn"), Some("relay,Trigger,,0,-1"), "the server keeps the wiring");
    for i in (0..LUMP_COUNT).filter(|i| *i != lump::ENTITIES && *i != lump::GAME_LUMP) {
        assert_eq!(
            client_bsp.lump_raw(i).expect("client lump"),
            server_bsp.lump_raw(i).expect("server lump"),
            "lump {i} differs between the two copies"
        );
    }
    assert_eq!(resolved_props(&server_bsp), resolved_props(&full), "the game lump holds the same props");
    assert_eq!(resolved_props(&server_bsp), resolved_props(&client_bsp));
    assert_eq!(texture_names(&server_bsp), texture_names(&client_bsp));
}

#[test]
fn protection_can_be_turned_off_piece_by_piece() {
    let full = sample_bsp();
    let keep_all = ProtectOptions {
        strip_logic: false,
        strip_connections: false,
        strip_original_faces: false,
        randomize_materials: false,
        ..Default::default()
    };
    let (bytes, report) = protect(&full, &keep_all).expect("protect");
    let out = Bsp::from_vec(bytes).expect("read back");
    assert_eq!(classes(&out), classes(&full));
    assert_eq!(texture_names(&out), STOCK);
    assert!(!out.lump(lump::ORIGINAL_FACES).expect("original faces").is_empty());
    assert_eq!((report.entities_removed, report.connections_stripped, report.materials_randomized), (0, 0, 0));
    assert!(report.layout_seed.is_empty(), "no layout change means no seed to report");
    let ents = out.entities().expect("entities");
    let door = ents.iter().find(|e| e.classname() == "func_door").expect("door");
    assert_eq!(door.get("OnFullyOpen"), Some("counter,Add,1,0,-1"));
    assert_eq!(door.get("hammerid"), None, "the editor keys always go");
}
