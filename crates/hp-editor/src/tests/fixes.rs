use std::time::Instant;

use glam::DVec3;
use hp_geom::{Aabb, Plane};
use hp_vmf::{Entity, Map, Solid};

use super::{box_sides, cube, no_bounds, vmf};
use crate::{
    ClipKeep, Document, FaceRef, MaterialKind, ObjRef, Owner, Problem, ProblemAssets, ProblemContext, Severity,
};

fn find<'a>(problems: &'a [Problem], code: &str) -> Option<&'a Problem> {
    problems.iter().find(|p| p.code == code)
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-editor/scratch").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn solid_with_ids(id: u32, first: u32, min: DVec3, size: f64) -> Solid {
    let mut sides = cube(min, size);
    for (k, s) in sides.iter_mut().enumerate() {
        s.id = first + k as u32;
    }
    Solid { id, sides, ..Solid::default() }
}

fn overlay(id: u32, sides: &str) -> Entity {
    let mut e = Entity { id, classname: "info_overlay".into(), ..Entity::default() };
    e.set("material", "overlays/rug001a");
    e.set("sides", sides);
    e.set("origin", "32 32 64");
    e
}

fn overlay_sides(doc: &Document, id: u32) -> String {
    doc.scene().entity(id).expect("overlay").entity.get("sides").unwrap_or_default().to_owned()
}

fn side_ids(doc: &Document, solid: u32) -> Vec<u32> {
    doc.scene().solid(solid).expect("solid").solid.sides.iter().map(|s| s.id).collect()
}

#[test]
fn clipping_updates_overlay_faces() {
    let mut map = Map::new_empty();
    map.world.solids.push(solid_with_ids(2, 10, DVec3::ZERO, 64.0));
    map.entities.push(overlay(50, "10"));
    let vertical = Plane::new(DVec3::X, 32.0).expect("plane");

    let mut doc = Document::from_map(map.clone());
    let before = vmf(&doc);
    let ids = doc.clip(&[2], &vertical, ClipKeep::Both).expect("clip");
    assert_eq!(ids.len(), 2);
    let back_top = 10;
    let front = doc.scene().solid(ids[1]).expect("front");
    let front_top = front
        .solid
        .sides
        .iter()
        .find(|s| Plane::from_side(s).is_some_and(|p| p.normal == DVec3::Z))
        .expect("front top")
        .id;
    assert!(side_ids(&doc, 2).contains(&back_top));
    assert_eq!(overlay_sides(&doc, 50), format!("{back_top} {front_top}"));
    assert!(doc.undo());
    assert_eq!(vmf(&doc), before, "undo restores the overlay too");

    let mut doc = Document::from_map(map.clone());
    doc.clip(&[2], &vertical, ClipKeep::Front).expect("clip");
    let kept = side_ids(&doc, 2);
    assert!(kept.contains(&10), "top face keeps its id: {kept:?}");
    assert_eq!(kept.iter().filter(|id| (10..16).contains(*id)).count(), 5, "only the cap is new: {kept:?}");
    assert_eq!(overlay_sides(&doc, 50), "10");

    let mut map2 = map.clone();
    map2.entities[0].set("sides", "10 15");
    let mut doc = Document::from_map(map2);
    doc.clip(&[2], &vertical, ClipKeep::Front).expect("clip");
    assert_eq!(overlay_sides(&doc, 50), "10");
}

#[test]
fn invalid_solids_do_not_blow_up_bounds() {
    let mut map = Map::new_empty();
    map.world.solids.push(solid_with_ids(2, 10, DVec3::ZERO, 128.0));
    let mut three = solid_with_ids(3, 20, DVec3::new(256.0, 0.0, 0.0), 64.0);
    three.sides.truncate(3);
    map.world.solids.push(three);
    let mut doc = Document::from_map(map);
    let stats = doc.stats();
    assert_eq!(stats.invalid_solids, 1);
    assert_eq!(stats.bounds, Some(Aabb::new(DVec3::ZERO, DVec3::splat(128.0))), "framing ignores the broken brush");
    doc.select_all();
    let b = doc.selection_bounds(&no_bounds).expect("bounds");
    assert!(b.max.max_element() <= 320.0 + 1e-6 && b.min.min_element() >= -1e-6, "selection bounds {b:?}");
    let p = doc.check_problems(None);
    assert!(find(&p, "invalid_solid").is_some());
}

#[test]
fn large_selections_are_linear() {
    let n = 30_000u32;
    let mut map = Map::new_empty();
    for i in 0..n {
        let x = f64::from(i % 200) * 16.0;
        let y = f64::from(i / 200) * 16.0;
        let mut s = Solid {
            id: i + 2,
            sides: box_sides(DVec3::new(x, y, 0.0), DVec3::new(x + 8.0, y + 8.0, 8.0), "A"),
            ..Solid::default()
        };
        for (k, side) in s.sides.iter_mut().enumerate() {
            side.id = i * 6 + k as u32 + 1;
        }
        map.world.solids.push(s);
    }
    let mut doc = Document::from_map(map);
    doc.select_all();
    let objs = doc.selected_objects();
    assert_eq!(objs.len(), n as usize);
    let t = Instant::now();
    doc.deselect(&objs);
    assert!(doc.selection().is_empty());
    doc.select(&objs, crate::SelectOp::Replace);
    doc.toggle_selection(&objs[..n as usize / 2]);
    assert_eq!(doc.selection().objects.len(), n as usize / 2);
    assert_eq!(doc.selected_objects().first(), objs.get(n as usize / 2), "order of the rest is kept");
    let elapsed = t.elapsed();
    assert!(elapsed.as_secs_f64() < 1.0, "deselecting {n} objects took {elapsed:?}");
    doc.select_all();
    assert_eq!(doc.hide_selection(), n as usize);
    assert!(doc.selection().is_empty());
    doc.unhide_all();
    doc.select_all();
    assert_eq!(doc.delete_selection().expect("delete"), n as usize);
    assert!(doc.selection().is_empty());
}

#[test]
fn nudging_another_object_starts_a_new_undo_step() {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 32.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::new(64.0, 0.0, 0.0), 32.0), Owner::World).expect("b");
    doc.clear_history();
    let mut limits = doc.undo_limits();
    limits.merge_window = None;
    doc.set_undo_limits(limits);
    doc.nudge(&[ObjRef::Solid(a)], DVec3::X * 16.0).expect("nudge a");
    doc.nudge(&[ObjRef::Solid(b)], DVec3::Y * 16.0).expect("nudge b");
    assert_eq!(doc.history_len(), (2, 0));
    assert!(doc.undo());
    let min = |doc: &Document, id| doc.scene().solid(id).expect("solid").bounds.min;
    assert_eq!(min(&doc, a), DVec3::new(16.0, 0.0, 0.0), "the first nudge stays");
    assert_eq!(min(&doc, b), DVec3::new(64.0, 0.0, 0.0));
    assert!(doc.redo());
    doc.nudge(&[ObjRef::Solid(a), ObjRef::Solid(b)], DVec3::Z).expect("both");
    doc.nudge(&[ObjRef::Solid(b), ObjRef::Solid(a)], DVec3::Z).expect("both");
    assert_eq!(doc.history_len(), (3, 0));
}

fn import_source() -> Map {
    let mut map = Map::new_empty();
    map.visgroups = vec![hp_vmf::VisGroup {
        name: "Lights".into(),
        id: 1,
        color: [255, 200, 0],
        children: vec![hp_vmf::VisGroup { name: "Spots".into(), id: 2, color: [0, 255, 0], ..Default::default() }],
        ..Default::default()
    }];
    for (i, x) in [(2u32, 0.0), (3, 128.0)] {
        let mut s = solid_with_ids(i, i * 10, DVec3::new(x, 0.0, 0.0), 64.0);
        s.editor.group_id = Some(20);
        map.world.solids.push(s);
    }
    map.world.groups.push(hp_vmf::Group { id: 20, ..Default::default() });
    let mut light = Entity { id: 5, classname: "light".into(), hidden: true, ..Entity::default() };
    light.set("origin", "64 64 32");
    light.editor.visgroup_ids = vec![1, 2];
    map.entities.push(light);
    map
}

#[test]
fn importing_a_map_keeps_groups_visgroups_and_hidden_state() {
    let mut doc = Document::new_empty();
    let logic = doc.create_visgroup("Logic", None, &[]).expect("visgroup");
    let objs = doc.import_map(import_source()).expect("import");
    assert_eq!(objs.len(), 3);
    assert_eq!(doc.scene().groups().len(), 1);
    let solid = objs[0];
    assert_eq!(doc.resolve(solid, crate::SelectMode::Groups).len(), 2);
    let lights = doc.scene().visgroups().iter().find(|v| v.name == "Lights").expect("Lights");
    let spots = lights.children.iter().find(|v| v.name == "Spots").expect("Spots");
    let light = objs.iter().find(|o| o.is_entity()).copied().expect("light");
    let ent = &doc.scene().entity(light.id()).expect("light").entity;
    assert_eq!(ent.editor.visgroup_ids, vec![lights.id, spots.id]);
    assert!(!ent.editor.visgroup_ids.contains(&logic));
    assert!(ent.hidden, "hidden objects stay hidden");
    assert!(!doc.selection().contains(light), "hidden objects are not selected");
    let count = doc.scene().visgroups().len();
    doc.import_map(import_source()).expect("again");
    assert_eq!(doc.scene().visgroups().len(), count);
    assert!(doc.undo());
    assert!(doc.undo());
    assert_eq!(doc.scene().visgroups().len(), 1);
    assert!(doc.scene().solids().is_empty());
}

#[test]
fn pasting_a_whole_vmf_recreates_groups() {
    let text = hp_vmf::write(&import_source());
    let mut doc = Document::new_empty();
    let objs = doc.paste(&text, DVec3::ZERO).expect("paste");
    assert_eq!(objs.len(), 3);
    assert_eq!(doc.scene().groups().len(), 1);
    for o in objs.iter().filter(|o| o.is_solid()) {
        assert!(doc.scene().object_group(*o).is_some(), "pasted solids keep their group");
    }
    let lights = doc.scene().visgroups().iter().find(|v| v.name == "Lights").expect("Lights");
    let light = objs.iter().find(|o| o.is_entity()).expect("light");
    assert!(doc.scene().entity(light.id()).expect("light").entity.editor.visgroup_ids.contains(&lights.id));
}

#[test]
fn new_problem_checks() {
    let mut map = Map::new_empty();
    map.world.solids.push(Solid {
        id: 2,
        sides: box_sides(DVec3::splat(-512.0), DVec3::splat(512.0), "DEV/A"),
        ..Solid::default()
    });
    let mut dup = solid_with_ids(3, 30, DVec3::ZERO, 32.0);
    let first = dup.sides[0].clone();
    dup.sides.push(hp_vmf::Side { id: 36, ..first });
    map.world.solids.push(dup);
    let mut water = solid_with_ids(4, 40, DVec3::new(64.0, 0.0, 0.0), 32.0);
    water.sides[0].material = "NATURE/WATER_CANALS03".into();
    water.sides[1].material = "NOSUCH/MISSING".into();
    map.world.solids.push(water);
    let mut spawn = Entity { id: 10, classname: "info_player_deathmatch".into(), ..Entity::default() };
    spawn.set("origin", "0 0 0");
    map.entities.push(spawn);
    let mut laser = Entity { id: 11, classname: "env_laser".into(), ..Entity::default() };
    laser.set("LaserTarget", "no_such_target");
    laser.set("origin", "1 1 1");
    map.entities.push(laser);
    map.entities.push(overlay(12, "999991 999992"));
    map.entities.push(overlay(13, "30 999993"));
    let mut prop = Entity { id: 14, classname: "prop_static".into(), ..Entity::default() };
    prop.set("model", "models/does/not/exist.mdl");
    prop.set("origin", "2 2 2");
    map.entities.push(prop);
    let mut doc = Document::from_map(map);

    let exists = |c: &str| c != "info_player_deathmatch";
    let model_exists = |m: &str| !m.contains("does/not");
    let kind = |m: &str| {
        Some(match m.to_ascii_lowercase().as_str() {
            "nature/water_canals03" => MaterialKind::Water,
            "nosuch/missing" => MaterialKind::Missing,
            _ => MaterialKind::Regular,
        })
    };
    let ctx = ProblemContext { class_exists: Some(&exists), is_brush_class: None };
    let assets = ProblemAssets { model_exists: Some(&model_exists), material_kind: Some(&kind), is_target_key: None };
    let problems = doc.check_problems_full(&ctx, &assets);
    assert!(find(&problems, "unknown_class").is_none(), "spawn classes are known");
    assert!(find(&problems, "no_player_spawn").is_none());
    let p = find(&problems, "duplicate_plane").expect("duplicate plane");
    assert_eq!(p.faces, vec![FaceRef::new(3, 6)]);
    let p = find(&problems, "missing_target").expect("missing target");
    assert_eq!(p.objects, vec![ObjRef::Entity(11)]);
    assert!(p.message.contains("no_such_target"));
    let overlays: Vec<&Problem> = problems.iter().filter(|p| p.code == "overlay_missing_faces").collect();
    assert_eq!(overlays.len(), 2);
    assert_eq!(overlays.iter().find(|p| p.objects == [ObjRef::Entity(12)]).map(|p| p.severity), Some(Severity::Error));
    let p = find(&problems, "missing_model").expect("model");
    assert_eq!((p.severity, p.objects.clone()), (Severity::Error, vec![ObjRef::Entity(14)]));
    let p = find(&problems, "missing_material").expect("material");
    assert_eq!(p.faces, vec![FaceRef::new(4, 1)]);
    assert_eq!(find(&problems, "mixed_face_contents").expect("mixed").objects, vec![ObjRef::Solid(4)]);
    let basic = doc.check_problems_with(&ctx);
    assert!(find(&basic, "missing_material").is_none() && find(&basic, "missing_model").is_none());

    let fixable: Vec<Problem> = problems.iter().filter(|p| p.fixable).cloned().collect();
    doc.fix_problems(&fixable).expect("fix");
    assert_eq!(doc.scene().solid(3).expect("solid").solid.sides.len(), 6);
    assert!(doc.scene().entity(12).is_none());
    assert_eq!(overlay_sides(&doc, 13), "30");
    let after = doc.check_problems_full(&ctx, &assets);
    assert!(find(&after, "duplicate_plane").is_none() && find(&after, "overlay_missing_faces").is_none());
    assert!(find(&after, "missing_target").is_none());
    assert!(
        doc.scene().entity(11).expect("entity").entity.props.iter().all(|(_, v)| !v.contains("no_such_target")),
        "the missing name is gone"
    );
}

#[test]
fn sample_maps_have_no_false_positives() {
    for path in super::sample_vmfs() {
        let doc = Document::open(&path).expect("open");
        assert!(doc.load_warnings().is_empty(), "{}: {:?}", path.display(), doc.load_warnings());
        let exists = |c: &str| !c.eq_ignore_ascii_case("info_player_deathmatch");
        let problems = doc.check_problems(Some(&exists));
        for code in
            ["duplicate_plane", "missing_target", "overlay_missing_faces", "solid_out_of_bounds", "missing_model"]
        {
            assert!(find(&problems, code).is_none(), "{}: {:?}", path.display(), find(&problems, code));
        }
        let unknown: Vec<&str> =
            problems.iter().filter(|p| p.code == "unknown_class").map(|p| p.message.as_str()).collect();
        assert!(unknown.iter().all(|m| !m.contains("info_player_deathmatch")), "{unknown:?}");
    }
}

#[test]
fn damaged_files_open_without_a_path() {
    let dir = scratch("damaged");
    let mut map = Map::new_empty();
    map.world.solids.push(solid_with_ids(2, 10, DVec3::ZERO, 64.0));
    let mut e = Entity { id: 3, classname: "info_target".into(), ..Entity::default() };
    e.set("origin", "0 0 0");
    map.entities.push(e);
    let text = hp_vmf::write(&map);
    let cut = text.find("\"plane\"").expect("plane") + 12;
    let path = dir.join("cut.vmf");
    std::fs::write(&path, &text.as_bytes()[..cut]).expect("write");
    let doc = Document::open(&path).expect("open");
    assert!(doc.path().is_none(), "Ctrl+S must not overwrite the damaged file");
    assert_eq!(doc.opened_from(), Some(path.as_path()));
    assert!(doc.is_dirty());
    assert!(doc.load_report().is_some_and(hp_vmf::LoadReport::is_damaged));
    assert!(!doc.load_warnings().is_empty());

    let good = dir.join("good.vmf");
    std::fs::write(&good, &text).expect("write");
    let doc = Document::open(&good).expect("open");
    assert_eq!(doc.path(), Some(good.as_path()));
    assert!(doc.load_warnings().is_empty());
    assert!(!doc.is_dirty());
}

#[test]
fn windows_1252_crlf_maps_save_with_the_same_bytes() {
    let dir = scratch("cp1252");
    let mut map = Map::new_empty();
    map.world.solids.push(solid_with_ids(2, 10, DVec3::ZERO, 64.0));
    for (id, name) in [(3, "porte_\u{e9}"), (4, "porte_\u{e8}")] {
        let mut e = Entity { id, classname: "game_text".into(), ..Entity::default() };
        e.set("targetname", name);
        e.set("message", "D\u{e9}j\u{e0} vu \u{2019}");
        e.set("origin", "0 0 0");
        map.entities.push(e);
    }
    map.version_info.map_version = 3;
    let text = hp_vmf::write(&map).replace('\n', "\r\n");
    let bytes = hp_kv::encode_windows_1252(&text).expect("cp1252");
    let path = dir.join("cp1252.vmf");
    std::fs::write(&path, &bytes).expect("write");
    let mut doc = Document::open(&path).expect("open");
    assert!(doc.load_warnings().is_empty(), "{:?}", doc.load_warnings());
    let names: Vec<String> =
        doc.scene().entities().values().filter_map(|n| n.entity.get("targetname").map(str::to_owned)).collect();
    assert_eq!(names, ["porte_\u{e9}", "porte_\u{e8}"]);
    let out = dir.join("out.vmf");
    doc.save(&out).expect("save");
    let expected =
        hp_kv::encode_windows_1252(&text.replace("\"mapversion\" \"3\"", "\"mapversion\" \"4\"")).expect("cp1252");
    assert!(std::fs::read(&out).expect("read") == expected, "only the map version changes");
}

#[test]
fn objects_without_id_get_one() {
    let mut map = Map::new_empty();
    map.world.solids.push(solid_with_ids(0, 10, DVec3::ZERO, 64.0));
    map.world.solids.push(solid_with_ids(7, 20, DVec3::new(100.0, 0.0, 0.0), 64.0));
    map.entities.push(Entity { id: 0, classname: "info_target".into(), ..Entity::default() });
    let doc = Document::from_map(map);
    assert!(doc.scene().solids().keys().all(|id| *id != 0));
    assert!(doc.scene().entities().keys().all(|id| *id != 0));
    assert!(doc.scene().solid(7).is_some());
}

#[test]
fn displacement_tags_on_walls() {
    let mut doc = Document::new_empty();
    let id = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("cube");
    doc.create_displacements(&[FaceRef::new(id, 0), FaceRef::new(id, 2)], 2).expect("disp");
    let n = doc.scene().solid(id).expect("solid");
    let tags = |i: usize| n.solid.sides[i].disp.as_ref().expect("disp").triangle_tags.clone();
    assert!(tags(0).iter().all(|&t| t == 9));
    assert!(tags(2).iter().all(|&t| t == 0));
}

fn point(id: u32, class: &str, keys: &[(&str, &str)]) -> Entity {
    let mut e = Entity { id, classname: class.into(), ..Entity::default() };
    e.set("origin", "32 32 32");
    for (k, v) in keys {
        e.set(k, *v);
    }
    e
}

fn room_map() -> Map {
    let mut map = Map::new_empty();
    map.world.solids.push(Solid {
        id: 2,
        sides: box_sides(DVec3::splat(-512.0), DVec3::splat(512.0), "DEV/A"),
        ..Solid::default()
    });
    map.entities.push(point(3, "info_player_start", &[]));
    map
}

#[test]
fn parent_names_with_an_attachment_are_valid() {
    let mut map = room_map();
    map.entities.push(point(9601, "prop_dynamic", &[("targetname", "qa_door"), ("model", "models/a/door.mdl")]));
    map.entities.push(point(9602, "prop_dynamic", &[("parentname", "qa_door,handle"), ("model", "models/a/b.mdl")]));
    map.entities.push(point(
        9603,
        "prop_dynamic",
        &[("parentname", " QA_DOOR , handle "), ("model", "models/a/b.mdl")],
    ));
    map.entities.push(point(9604, "prop_dynamic", &[("parentname", "nobody,handle"), ("model", "models/a/b.mdl")]));
    map.entities.push(point(9605, "env_laser", &[("LaserTarget", "qa_door,handle")]));
    let doc = Document::from_map(map);
    let problems = doc.check_problems(None);
    let bad: Vec<ObjRef> =
        problems.iter().filter(|p| p.code == "missing_target").flat_map(|p| p.objects.clone()).collect();
    assert_eq!(bad, vec![ObjRef::Entity(9604), ObjRef::Entity(9605)], "{problems:?}");
    let p = problems.iter().find(|p| p.objects == [ObjRef::Entity(9604)]).expect("missing parent");
    assert!(p.message.contains("parentname = 'nobody,handle'"), "{}", p.message);
    assert_eq!(crate::parent_name("qa_door,handle"), "qa_door");
    assert_eq!(crate::parent_name(" qa_door "), "qa_door");
    assert_eq!(crate::parent_name(""), "");
}

#[test]
fn brushes_outside_the_map_limit() {
    let mut map = room_map();
    map.world.solids.push(Solid { id: 299, sides: cube(DVec3::splat(16320.0), 64.0), ..Solid::default() });
    map.world.solids.push(Solid { id: 300, sides: cube(DVec3::new(16000.0, 0.0, 0.0), 512.0), ..Solid::default() });
    map.world.solids.push(Solid { id: 301, sides: cube(DVec3::new(-20000.0, 0.0, 0.0), 64.0), ..Solid::default() });
    let mut door = point(400, "func_door", &[]);
    door.solids.push(Solid { id: 401, sides: cube(DVec3::new(0.0, 0.0, -16400.0), 32.0), ..Solid::default() });
    map.entities.push(door);
    let doc = Document::from_map(map);
    let problems = doc.check_problems(None);
    let p = find(&problems, "solid_out_of_bounds").expect("brushes outside");
    assert_eq!(p.objects, vec![ObjRef::Solid(300), ObjRef::Solid(301), ObjRef::Solid(401)]);
    assert_eq!(p.severity, Severity::Error);
    assert!(p.message.starts_with("3 solids are outside the maximum map size"), "{}", p.message);
    assert!(find(&problems, "invalid_solid").is_none());
    assert!(find(&Document::from_map(room_map()).check_problems(None), "solid_out_of_bounds").is_none());
}

#[test]
fn props_without_a_usable_model() {
    let mut map = room_map();
    let props: [(u32, &str, &str); 9] = [
        (971, "prop_static", "models/does/not/exist.mdl"),
        (972, "prop_static", ""),
        (973, "prop_static", "../../../../../../Windows/System32/notepad.exe"),
        (974, "prop_static", r"MODELS\props_c17\oildrum001.mdl"),
        (975, "prop_static", "materials/dev/dev_measuregeneric01.vmt"),
        (976, "prop_static", r"C:\Windows\win.ini"),
        (977, "prop_physics", "models/../models/props_c17/oildrum001.mdl"),
        (978, "prop_dynamic", "props_c17/oildrum001.mdl"),
        (979, "prop_static", "models/props_c17/oildrum001.mdl"),
    ];
    for (id, class, model) in props {
        map.entities.push(point(id, class, &[("model", model)]));
    }
    map.entities.push(point(980, "prop_dynamic", &[]));
    map.entities.push(point(981, "env_sprite", &[("model", "sprites/glow01.vmt")]));
    map.entities.push(point(982, "info_target", &[]));
    map.entities.push(point(983, "npc_citizen", &[("model", "C:/models/x.mdl")]));
    let doc = Document::from_map(map);
    let models = |problems: &[Problem]| -> Vec<u32> {
        problems.iter().filter(|p| p.code == "missing_model").flat_map(|p| p.objects.iter().map(|o| o.id())).collect()
    };

    let problems = doc.check_problems(None);
    assert_eq!(models(&problems), [972, 973, 975, 976, 977, 978, 980, 983], "{problems:?}");
    let empty = problems.iter().find(|p| p.objects == [ObjRef::Entity(972)]).expect("empty");
    assert_eq!(empty.severity, Severity::Error, "vbsp fails on a static prop without a model");
    assert!(empty.message.ends_with("does not exist: \"\""), "{}", empty.message);
    let exe = problems.iter().find(|p| p.objects == [ObjRef::Entity(973)]).expect("exe");
    assert!(exe.message.ends_with(": ../../../../../../Windows/System32/notepad.exe"), "{}", exe.message);
    let dynamic = problems.iter().find(|p| p.objects == [ObjRef::Entity(978)]).expect("dynamic");
    assert_eq!(dynamic.severity, Severity::Warning);

    let mounted = |m: &str| m.eq_ignore_ascii_case("models/props_c17/oildrum001.mdl");
    let assets = ProblemAssets { model_exists: Some(&mounted), material_kind: None, is_target_key: None };
    let problems = doc.check_problems_full(&ProblemContext::default(), &assets);
    assert_eq!(models(&problems), [971, 972, 973, 975, 976, 977, 978, 980, 983], "{problems:?}");
}

fn listed_ids(doc: &Document, overlay: u32) -> Vec<u32> {
    overlay_sides(doc, overlay).split_ascii_whitespace().filter_map(|t| t.parse().ok()).collect()
}

#[test]
fn union_and_merge_update_overlay_faces() {
    let mut map = Map::new_empty();
    map.world.solids.push(solid_with_ids(2, 10, DVec3::ZERO, 64.0));
    map.world.solids.push(solid_with_ids(3, 20, DVec3::new(32.0, 32.0, 0.0), 64.0));
    map.entities.push(overlay(50, "10 11 12 13 14 15 20 21 22 23 24 25"));
    let mut doc = Document::from_map(map);
    let before = vmf(&doc);
    let ids = doc.union_solids(&[2, 3]).expect("union");
    let alive: Vec<u32> = ids.iter().flat_map(|id| side_ids(&doc, *id)).collect();
    let listed = listed_ids(&doc, 50);
    assert!(!listed.is_empty());
    assert!(listed.iter().all(|id| alive.contains(id)), "{listed:?} vs {alive:?}");
    assert!(doc.undo());
    assert_eq!(vmf(&doc), before);

    let mut map = Map::new_empty();
    map.world.solids.push(solid_with_ids(2, 10, DVec3::ZERO, 64.0));
    map.world.solids.push(solid_with_ids(3, 20, DVec3::new(64.0, 0.0, 0.0), 64.0));
    map.entities.push(overlay(50, "10 11 12 13 14 15 20 21 22 23 24 25"));
    let mut doc = Document::from_map(map);
    doc.merge_brushes(&[2, 3]).expect("merge");
    let alive = side_ids(&doc, 2);
    let listed = listed_ids(&doc, 50);
    assert!(!listed.is_empty() && listed.len() < 12);
    assert!(listed.iter().all(|id| alive.contains(id)), "{listed:?} vs {alive:?}");
}
