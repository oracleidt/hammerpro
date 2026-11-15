use std::path::PathBuf;

use glam::DVec3;
use hp_vmf::{
    Connection, Cordon, CordonStyle, DispInfo, Entity, Map, SaveOptions, Side, Solid, TexAxis, VmfError, parse,
    parse_fragment, write,
};

fn scratch_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-vmf/scratch").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cube(id: u32, first_side: u32, min: DVec3, max: DVec3) -> Solid {
    let (a, b) = (min, max);
    let planes = [
        [DVec3::new(a.x, b.y, b.z), DVec3::new(b.x, b.y, b.z), DVec3::new(b.x, a.y, b.z)],
        [DVec3::new(a.x, a.y, a.z), DVec3::new(b.x, a.y, a.z), DVec3::new(b.x, b.y, a.z)],
        [DVec3::new(a.x, b.y, b.z), DVec3::new(a.x, a.y, b.z), DVec3::new(a.x, a.y, a.z)],
        [DVec3::new(b.x, b.y, a.z), DVec3::new(b.x, a.y, a.z), DVec3::new(b.x, a.y, b.z)],
        [DVec3::new(b.x, b.y, b.z), DVec3::new(a.x, b.y, b.z), DVec3::new(a.x, b.y, a.z)],
        [DVec3::new(b.x, a.y, a.z), DVec3::new(a.x, a.y, a.z), DVec3::new(a.x, a.y, b.z)],
    ];
    Solid {
        id,
        sides: planes
            .iter()
            .enumerate()
            .map(|(i, p)| Side {
                id: first_side + i as u32,
                plane: *p,
                material: "DEV/DEV_MEASUREGENERIC01".into(),
                ..Side::default()
            })
            .collect(),
        ..Solid::default()
    }
}

#[test]
fn new_empty_writes_hammer_layout() {
    let map = Map::new_empty();
    let text = write(&map);
    let want = "versioninfo\n{\n\t\"editorversion\" \"400\"\n\t\"editorbuild\" \"9540\"\n\t\"mapversion\" \"1\"\n\
\t\"formatversion\" \"100\"\n\t\"prefab\" \"0\"\n}\nvisgroups\n{\n}\nviewsettings\n{\n\t\"bSnapToGrid\" \"1\"\n\
\t\"bShowGrid\" \"1\"\n\t\"bShowLogicalGrid\" \"0\"\n\t\"nGridSpacing\" \"64\"\n\t\"bShow3DGrid\" \"0\"\n}\nworld\n{\n\
\t\"id\" \"1\"\n\t\"mapversion\" \"1\"\n\t\"classname\" \"worldspawn\"\n\t\"detailmaterial\" \"detail/detailsprites\"\n\
\t\"detailvbsp\" \"detail.vbsp\"\n\t\"maxpropscreenwidth\" \"-1\"\n\t\"skyname\" \"sky_day01_01\"\n}\ncameras\n{\n\
\t\"activecamera\" \"-1\"\n}\ncordon\n{\n\t\"mins\" \"(-1024 -1024 -1024)\"\n\t\"maxs\" \"(1024 1024 1024)\"\n\
\t\"active\" \"0\"\n}\n";
    assert_eq!(text, want);
    let back = parse(&text).unwrap();
    assert_eq!(back, map, "a new map reads back exactly");
    assert!(write(&Map::default()).contains("cordon\n{\n\t\"mins\" \"(-1024 -1024 -1024)\""));
    assert_eq!(map.max_object_id(), 1);
    assert_eq!(map.max_side_id(), 0);
}

#[test]
fn solid_layout_and_normals() {
    let s = cube(2, 1, DVec3::splat(-64.0), DVec3::new(64.0, 64.0, 0.5));
    assert_eq!(s.sides[0].normal(), DVec3::Z);
    assert_eq!(s.sides[1].normal(), DVec3::NEG_Z);
    assert_eq!(s.sides[2].normal(), DVec3::NEG_X);
    assert_eq!(s.sides[3].normal(), DVec3::X);
    assert_eq!(s.sides[4].normal(), DVec3::Y);
    assert_eq!(s.sides[5].normal(), DVec3::NEG_Y);
    assert_eq!(s.sides[0].plane_distance(), 0.5);
    let text = hp_vmf::write_solid(&s);
    assert!(text.starts_with(
        "solid\n{\n\t\"id\" \"2\"\n\tside\n\t{\n\t\t\"id\" \"1\"\n\t\t\"plane\" \"(-64 64 0.5) (64 64 0.5) (64 -64 0.5)\"\n\
\t\t\"material\" \"DEV/DEV_MEASUREGENERIC01\"\n\t\t\"uaxis\" \"[1 0 0 0] 0.25\"\n\t\t\"vaxis\" \"[0 -1 0 0] 0.25\"\n\
\t\t\"rotation\" \"0\"\n\t\t\"lightmapscale\" \"16\"\n\t\t\"smoothing_groups\" \"0\"\n\t}\n"
    ));
    assert!(text.ends_with(
        "\teditor\n\t{\n\t\t\"color\" \"0 180 255\"\n\t\t\"visgroupshown\" \"1\"\n\t\t\"visgroupautoshown\" \"1\"\n\t}\n}\n"
    ));
}

#[test]
fn hidden_groups_and_extras_round_trip() {
    let src = r#"versioninfo { "editorversion" "400" "custom" "x" }
world
{
	"id" "1"
	"mapversion" "3"
	"classname" "worldspawn"
	"skyname" "sky_day01_01"
	solid { "id" "2" side { "id" "1" "plane" "(0 0 1) (1 0 1) (1 1 1)" } editor { "color" "1 2 3" "groupid" "9" "visgroupid" "4" } }
	hidden { solid { "id" "3" side { "id" "2" "plane" "(0 0 1) (1 0 1) (1 1 1)" } } }
	group { "id" "9" editor { "color" "5 6 7" } }
	world_plus { "k" "v" }
}
hidden
{
	entity
	{
		"id" "10"
		"classname" "func_detail"
		hidden { solid { "id" "11" } }
		solid { "id" "12" }
	}
}
entity { "id" "13" "classname" "info_target" "targetname" "t" connections { "OnUser1" "t,Kill,,0,1" } }
palette_plus { "color0" "255 255 255" }
"#;
    let map = parse(src).unwrap();
    assert_eq!(map.version_info.map_version, 1, "versioninfo has no mapversion, the world one is ignored");
    assert_eq!(map.version_info.extra_props, vec![("custom".to_owned(), "x".to_owned())]);
    assert_eq!(map.world.solids.len(), 2);
    assert!(!map.world.solids[0].hidden && map.world.solids[1].hidden);
    assert_eq!(map.world.solids[0].editor.group_id, Some(9));
    assert_eq!(map.world.solids[0].editor.visgroup_ids, vec![4]);
    assert_eq!(map.world.groups.len(), 1);
    assert_eq!(map.world.groups[0].editor.color, [5, 6, 7]);
    assert_eq!(map.world.extra_blocks[0].name, "world_plus");
    assert_eq!(map.entities.len(), 2);
    assert!(map.entities[0].hidden && !map.entities[1].hidden);
    assert_eq!(map.entities[0].solids.iter().map(|s| (s.id, s.hidden)).collect::<Vec<_>>(), [(11, true), (12, false)]);
    assert_eq!(map.entities[1].connections[0].times, 1);
    assert_eq!(map.extra_blocks[0].name, "palette_plus");
    assert_eq!(map.max_object_id(), 13);
    assert_eq!(map.max_side_id(), 2);
    assert_eq!(map.solid_by_id(11).map(|s| s.hidden), Some(true));

    let text = write(&map);
    assert!(text.contains("\thidden\n\t{\n\t\tsolid\n\t\t{\n\t\t\t\"id\" \"3\"\n"));
    assert!(text.contains("hidden\n{\n\tentity\n\t{\n\t\t\"id\" \"10\"\n\t\t\"classname\" \"func_detail\"\n"));
    assert!(text.contains("\t\"id\" \"1\"\n\t\"mapversion\" \"1\"\n\t\"classname\" \"worldspawn\"\n"));
    assert!(text.ends_with("palette_plus\n{\n\t\"color0\" \"255 255 255\"\n}\n"));
    let back = parse(&text).unwrap();
    assert_eq!(back, map);
    assert_eq!(write(&back), text);
}

#[test]
fn displacement_round_trip() {
    let mut s = cube(1, 1, DVec3::ZERO, DVec3::splat(64.0));
    let mut d = DispInfo::new_flat(3, DVec3::new(0.0, 0.0, 64.0), DVec3::Z);
    for (i, x) in d.distances.iter_mut().enumerate() {
        *x = (i as f64 * 0.37).sin() * 16.0;
    }
    d.alphas[40] = 255.0;
    d.triangle_tags[3] = 1;
    s.sides[0].disp = Some(Box::new(d));
    s.sides[1].vertices_plus = Some(vec![DVec3::ZERO, DVec3::X, DVec3::new(0.1, 0.2, 0.3)]);
    let mut map = Map::new_empty();
    map.world.solids.push(s);
    let text = write(&map);
    let back = parse(&text).unwrap();
    assert_eq!(back, map);
    let side = &back.world.solids[0].sides[1];
    assert!(text.contains(
        "\t\t\t\"plane\" \"(0 0 0) (64 0 0) (64 64 0)\"\n\t\t\tvertices_plus\n\t\t\t{\n\t\t\t\t\"v\" \"0 0 0\"\n"
    ));
    assert_eq!(side.vertices_plus.as_ref().map(Vec::len), Some(3));
}

#[test]
fn cordon_writing_rules() {
    let mut map = Map::new_empty();
    assert_eq!(map.cordons.cordons.len(), 1, "new maps carry Hammer's default cordon box");
    map.cordons.cordons[0] =
        Cordon { name: "a".into(), active: true, mins: DVec3::splat(-8.0), maxs: DVec3::splat(8.0), exclude: false };
    map.cordons.active = true;
    let text = write(&map);
    assert!(text.contains("cordon\n{\n\t\"mins\" \"(-8 -8 -8)\"\n\t\"maxs\" \"(8 8 8)\"\n\t\"active\" \"1\"\n}\n"));
    map.cordons.cordons.push(Cordon {
        name: "b".into(),
        active: false,
        mins: DVec3::ZERO,
        maxs: DVec3::ONE,
        exclude: false,
    });
    let text = write(&map);
    assert!(text.contains(
        "cordons\n{\n\t\"active\" \"1\"\n\tcordon\n\t{\n\t\t\"name\" \"a\"\n\t\t\"active\" \"1\"\n\t\tbox\n\t\t{\n\
\t\t\t\"mins\" \"(-8 -8 -8)\"\n\t\t\t\"maxs\" \"(8 8 8)\"\n\t\t}\n\t}\n"
    ));
    let back = parse(&text).unwrap();
    assert_eq!(back.cordons.style, CordonStyle::Modern);
    assert_eq!(back.cordons.cordons, map.cordons.cordons);
    let bare = parse("world { \"id\" \"1\" \"classname\" \"worldspawn\" }").unwrap();
    assert!(write(&bare).contains("cordons\n{\n\t\"active\" \"0\"\n}\n"));
    assert_eq!(parse(&write(&bare)).unwrap(), bare);
}

#[test]
fn connections_api() {
    let c = Connection::parse("OnTrigger", "door,Open,,1.5,-1");
    assert_eq!(c.to_value_string(), "door,Open,,1.5,-1");
    let c = Connection::parse("OnTrigger", "door\x1bSetText\x1bhello, world\x1b0\x1b1");
    assert_eq!(c.param, "hello, world");
    assert_eq!(c.to_value_string(), "door\x1bSetText\x1bhello, world\x1b0\x1b1");
    let c = Connection { target: "a".into(), input: "AddOutput".into(), param: "x,y".into(), ..Connection::default() };
    assert_eq!(c.effective_separator(), '\x1b');
    assert_eq!(c.to_value_string(), "a\x1bAddOutput\x1bx,y\x1b0\x1b-1");
}

#[test]
fn fragments_round_trip() {
    let mut s = cube(5, 20, DVec3::ZERO, DVec3::ONE);
    s.hidden = true;
    let e = Entity {
        id: 6,
        classname: "func_detail".into(),
        props: vec![("targetname".into(), "d".into())],
        solids: vec![cube(7, 30, DVec3::ZERO, DVec3::splat(2.0))],
        ..Entity::default()
    };
    let text = format!("{}{}", hp_vmf::write_solid(&s), hp_vmf::write_entity(&e));
    let (solids, entities) = parse_fragment(&text).unwrap();
    assert_eq!(solids, vec![s.clone()]);
    assert_eq!(entities, vec![e.clone()]);

    let mut map = Map::new_empty();
    map.world.solids.push(cube(8, 40, DVec3::ZERO, DVec3::ONE));
    map.entities.push(e.clone());
    let (solids, entities) = parse_fragment(&write(&map)).unwrap();
    assert_eq!(solids.len(), 1);
    assert_eq!(entities, vec![e]);
    assert_eq!(parse_fragment("nothing here").unwrap(), (vec![], vec![]));
}

#[test]
fn errors_and_leniency() {
    assert!(matches!(parse(""), Err(VmfError::Parse(_))));
    assert!(matches!(parse("just some text"), Err(VmfError::Parse(_))));
    assert!(matches!(parse(&"world {".repeat(10_000)), Err(VmfError::Parse(_))));
    let mut noise = Vec::with_capacity(1 << 20);
    let mut state = 0x2545_f491_4f6c_dd1du64;
    while noise.len() < 1 << 20 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        noise.extend_from_slice(&state.to_le_bytes());
    }
    assert!(matches!(hp_vmf::parse_bytes(&noise), Err(VmfError::NotText)));
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x10";
    assert!(matches!(hp_vmf::parse_bytes(png), Err(VmfError::NotText)));
    assert!(matches!(parse("world { \"id\" \"1\" }\0\0\0\0"), Err(VmfError::NotText)));
    assert!(VmfError::NotText.to_string().contains("not a VMF text file"));
    match parse(&"a{ b{ c}\t".repeat(1000)) {
        Err(VmfError::Parse(m)) => assert!(m.contains("not a VMF document"), "{m}"),
        other => panic!("garbage text: {other:?}"),
    }
    let mut bytes = b"world { \"id\" \"1\" \"message\" \"caf".to_vec();
    bytes.push(0xE9);
    bytes.extend_from_slice(b"\" solid { \"id\" \"2\"");
    let (map, report) = hp_vmf::parse_bytes_with_report(&bytes).unwrap();
    assert_eq!(map.world.solids.len(), 1);
    assert_eq!(map.world.get("message"), Some("caf\u{e9}"));
    assert_eq!(map.format.encoding, hp_vmf::TextEncoding::Windows1252);
    assert_eq!(report.syntax.unclosed_blocks, 2);
    assert!(report.is_damaged());
    assert!(!report.warnings().is_empty());
    let map = parse("entity { \"id\" \"4\" \"classname\" \"light\" }").unwrap();
    assert_eq!(map.world.classname, "worldspawn");
    assert_eq!(map.entities[0].classname, "light");
}

#[test]
fn editor_defaults_and_world_editor() {
    let mut map = Map::new_empty();
    map.world.editor.color = [1, 2, 3];
    let text = write(&map);
    assert!(text.contains("\teditor\n\t{\n\t\t\"color\" \"1 2 3\"\n"));
    assert_eq!(parse(&text).unwrap().world.editor.color, [1, 2, 3]);
}

#[test]
fn texture_axes_keep_precision() {
    let mut s = cube(1, 1, DVec3::ZERO, DVec3::ONE);
    s.sides[0].uaxis = TexAxis {
        axis: DVec3::new(std::f64::consts::FRAC_1_SQRT_2, std::f64::consts::FRAC_1_SQRT_2, 0.0),
        shift: -12.25,
        scale: 0.1,
    };
    s.sides[0].rotation = 45.0;
    let mut map = Map::new_empty();
    map.world.solids.push(s);
    let text = write(&map);
    assert!(text.contains("\"uaxis\" \"[0.7071067811865476 0.7071067811865476 0 -12.25] 0.1\""));
    assert_eq!(parse(&text).unwrap(), map);
}

#[test]
fn save_is_atomic_and_keeps_vmx_backup() {
    let dir = scratch_dir("save");
    let path = dir.join("test_map.vmf");
    let mut map = Map::new_empty();
    hp_vmf::save(&map, &path).unwrap();
    assert!(!dir.join("test_map.vmx").exists(), "no backup for a new file");
    let first = std::fs::read_to_string(&path).unwrap();

    map.version_info.map_version = 2;
    let report = hp_vmf::save_with(&map, &path, &SaveOptions::default()).unwrap();
    assert_eq!(report.backup_path.as_deref(), Some(dir.join("test_map.vmx").as_path()));
    assert!(report.backup_error.is_none());
    assert_eq!(std::fs::read_to_string(dir.join("test_map.vmx")).unwrap(), first);
    assert_eq!(hp_vmf::load(&path).unwrap().version_info.map_version, 2);
    assert_eq!(report.bytes, std::fs::metadata(&path).unwrap().len() as usize);

    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty());

    assert!(hp_vmf::save(&map, &dir.join("missing").join("x.vmf")).is_err());
}

#[test]
fn entity_helpers() {
    let mut map = Map::new_empty();
    map.entities.push(Entity { id: 3, classname: "light".into(), ..Entity::default() });
    map.entity_by_id_mut(3).unwrap().set("origin", "1 2 3");
    assert_eq!(map.entity_by_id(3).unwrap().origin(), Some(DVec3::new(1.0, 2.0, 3.0)));
    assert!(map.entity_by_id(1).is_none(), "the world is not an entity");
    map.world.solids.push(cube(9, 1, DVec3::ZERO, DVec3::ONE));
    for s in map.all_solids_mut() {
        s.editor.color = [9, 9, 9];
    }
    assert!(map.all_solids().all(|s| s.editor.color == [9, 9, 9]));
}
