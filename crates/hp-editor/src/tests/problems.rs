use glam::DVec3;
use hp_vmf::{Connection, Entity, Map, Solid};

use super::{box_sides, cube, vmf};
use crate::{Document, FaceRef, ObjRef, Owner, Problem, ProblemContext, Severity};

fn find<'a>(problems: &'a [Problem], code: &str) -> Option<&'a Problem> {
    problems.iter().find(|p| p.code == code)
}

#[test]
fn empty_map() {
    let doc = Document::new_empty();
    let p = doc.check_problems(None);
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].code, "empty_map");
}

#[test]
fn detects_and_fixes() {
    let mut doc = Document::new_empty();
    let room = doc
        .create_solid(box_sides(DVec3::splat(-512.0), DVec3::splat(512.0), "DEV/DEV_MEASUREGENERIC01"), Owner::World)
        .expect("room");
    let trig = doc
        .create_solid(box_sides(DVec3::ZERO, DVec3::splat(32.0), "TOOLS/TOOLSTRIGGER"), Owner::World)
        .expect("trigger");
    let relay = doc.create_point_entity("logic_relay", DVec3::ZERO, &[]).expect("relay");
    doc.add_connection(&[relay], &Connection::parse("OnTrigger", "nobody,Kill,,0,-1")).expect("output");
    doc.add_connection(&[relay], &Connection::parse("OnTrigger", "!self,Kill,,0,-1")).expect("output");
    let lost = doc.create_point_entity("info_target", DVec3::new(2000.0, 0.0, 0.0), &[]).expect("outside");
    let far = doc.create_point_entity("info_target", DVec3::new(20000.0, 0.0, 0.0), &[]).expect("far");
    let weird = doc.create_point_entity("weird_class", DVec3::ZERO, &[]).expect("weird");
    let empty_door = doc.create_point_entity("func_door", DVec3::ZERO, &[]).expect("door");
    let mut s = doc.scene().solid(room).expect("room").solid.clone();
    s.sides[0].uaxis.axis = DVec3::Z;
    s.sides[0].vaxis.axis = DVec3::Z;
    doc.update_solid(room, s);

    let class_exists = |c: &str| c != "weird_class";
    let problems = doc.check_problems(Some(&class_exists));
    assert!(problems.windows(2).all(|w| w[0].severity >= w[1].severity));
    let p = find(&problems, "missing_output_target").expect("output");
    assert_eq!(p.objects, vec![ObjRef::Entity(relay)]);
    assert!(p.message.contains("nobody"));
    assert_eq!(find(&problems, "unknown_class").expect("class").objects, vec![ObjRef::Entity(weird)]);
    assert_eq!(find(&problems, "brush_entity_without_solids").expect("door").objects, vec![ObjRef::Entity(empty_door)]);
    assert_eq!(find(&problems, "tool_texture_on_world").expect("tool").objects, vec![ObjRef::Solid(trig)]);
    assert_eq!(find(&problems, "entity_outside_map").expect("outside").objects, vec![ObjRef::Entity(lost)]);
    assert_eq!(find(&problems, "entity_out_of_bounds").expect("bounds").objects, vec![ObjRef::Entity(far)]);
    assert!(find(&problems, "no_player_spawn").is_some());
    assert_eq!(find(&problems, "invalid_texture_axes").expect("axes").faces, vec![FaceRef::new(room, 0)]);
    assert_eq!(find(&problems, "brush_entity_without_solids").map(|p| p.severity), Some(Severity::Error));

    let before = vmf(&doc);
    let fixable: Vec<Problem> = problems.iter().filter(|p| p.fixable).cloned().collect();
    let n = doc.fix_problems(&fixable).expect("fix");
    assert_eq!(n, fixable.len());
    assert_eq!(doc.undo_label(), Some("Fix problems"));
    let after = doc.check_problems(Some(&class_exists));
    for code in ["missing_output_target", "brush_entity_without_solids", "invalid_texture_axes"] {
        assert!(find(&after, code).is_none(), "{code} still reported");
    }
    assert_eq!(doc.scene().entity(relay).expect("relay").entity.connections.len(), 1);
    assert!(doc.undo());
    assert_eq!(vmf(&doc), before, "fixes are one undo step");
    let unfixable = find(&problems, "no_player_spawn").expect("spawn").clone();
    assert!(!doc.fix_problem(&unfixable).expect("no fix"));
    let mut bogus = unfixable;
    bogus.fixable = true;
    bogus.code = "bogus";
    assert!(doc.fix_problem(&bogus).is_err());
    let never_brush = |_: &str| Some(false);
    let ctx = ProblemContext { class_exists: None, is_brush_class: Some(&never_brush) };
    assert!(find(&doc.check_problems_with(&ctx), "brush_entity_without_solids").is_none());
}

#[test]
fn structural_problems() {
    let mut map = Map::new_empty();
    let mut bad = Solid { id: 2, sides: cube(DVec3::ZERO, 16.0), ..Solid::default() };
    bad.sides.truncate(3);
    let mut wedge = Solid { id: 3, sides: cube(DVec3::new(100.0, 0.0, 0.0), 16.0), ..Solid::default() };
    let mut ids = 100u32;
    let w = hp_geom::make_primitive(
        &hp_geom::PrimitiveParams {
            kind: hp_geom::Primitive::Wedge,
            bounds: hp_geom::Aabb::new(DVec3::new(100.0, 0.0, 0.0), DVec3::new(164.0, 64.0, 64.0)),
            ..hp_geom::PrimitiveParams::default()
        },
        &mut || {
            ids += 1;
            ids
        },
    )
    .expect("wedge");
    wedge.sides = w.into_iter().next().expect("one solid");
    let geom = hp_geom::build_solid(&wedge.sides);
    let tri = geom.faces.iter().find(|f| f.vertices.len() == 3).expect("triangle").side_index;
    wedge.sides[tri].disp = Some(Box::new(hp_vmf::DispInfo::new_flat(2, DVec3::ZERO, DVec3::Z)));
    let mut good = Solid { id: 4, sides: cube(DVec3::new(0.0, 100.0, 0.0), 16.0), ..Solid::default() };
    good.editor.group_id = Some(777);
    good.editor.visgroup_ids = vec![55];
    map.world.solids = vec![bad, wedge, good];
    let mut e = Entity { id: 4, classname: "info_player_start".into(), ..Entity::default() };
    e.set("origin", "0 100 8");
    map.entities = vec![e];
    let mut doc = Document::from_map(map);
    let problems = doc.check_problems(None);
    assert_eq!(find(&problems, "invalid_solid").expect("invalid").objects, vec![ObjRef::Solid(2)]);
    assert_eq!(find(&problems, "disp_not_quad").expect("disp").faces, vec![FaceRef::new(3, tri)]);
    assert!(find(&problems, "duplicate_object_id").is_some());
    assert!(find(&problems, "duplicate_side_id").is_some());
    assert_eq!(find(&problems, "missing_group").expect("group").objects, vec![ObjRef::Solid(4)]);
    assert_eq!(find(&problems, "missing_visgroup").expect("visgroup").objects, vec![ObjRef::Solid(4)]);
    assert!(find(&problems, "no_player_spawn").is_none());
    let fixable: Vec<Problem> = problems.iter().filter(|p| p.fixable).cloned().collect();
    doc.fix_problems(&fixable).expect("fix");
    let after = doc.check_problems(None);
    for code in [
        "invalid_solid",
        "disp_not_quad",
        "duplicate_object_id",
        "duplicate_side_id",
        "missing_group",
        "missing_visgroup",
    ] {
        assert!(find(&after, code).is_none(), "{code} still reported: {after:?}");
    }
    assert_eq!(doc.scene().entities().len(), 1);
    assert_ne!(doc.scene().entities().keys().next().copied(), Some(4));
}

struct InstanceNames(Vec<String>);

impl crate::limits::checks::CheckKnowledge for InstanceNames {
    fn extra_target_names(&self) -> &[String] {
        &self.0
    }
}

#[test]
fn outputs_aimed_at_instance_contents_are_not_missing() {
    use crate::limits::checks::{CheckOptions, NoKnowledge, check_snapshot};
    use crate::limits::snapshot::MapSnapshot;

    let mut doc = Document::new_empty();
    doc.create_solid(box_sides(DVec3::splat(-512.0), DVec3::splat(512.0), "DEV/DEV_MEASUREGENERIC01"), Owner::World)
        .expect("room");
    let relay = doc.create_point_entity("logic_relay", DVec3::ZERO, &[("parentname".into(), "Room-Lift".into())]);
    let relay = relay.expect("relay");
    for target in ["room-door", "ROOM-LIGHT*", "nobody"] {
        let c = Connection::parse("OnTrigger", &format!("{target},Open,,0,-1"));
        doc.add_connection(&[relay], &c).expect("output");
    }
    let snap = MapSnapshot::capture(&doc);
    let opts = CheckOptions::default();
    let plain = check_snapshot(&snap, &NoKnowledge, &opts);
    let p = find(&plain, "missing_output_target").expect("missing");
    assert_eq!(p.args, vec!["nobody".to_owned(), "room-door".to_owned(), "room-light*".to_owned()]);
    assert!(find(&plain, "missing_target").is_some(), "parentname of an instance entity");
    let names = InstanceNames(vec!["room-door".into(), "room-lift".into(), "room-light_1".into()]);
    let known = check_snapshot(&snap, &names, &opts);
    let p = find(&known, "missing_output_target").expect("missing").clone();
    assert_eq!(p.args, vec!["nobody".to_owned()]);
    assert!(p.message.contains("nobody") && !p.message.contains("room-door"));
    assert!(find(&known, "missing_target").is_none());
    assert!(doc.fix_problem(&p).expect("fix"));
    let targets: Vec<String> =
        doc.scene().entity(relay).expect("relay").entity.connections.iter().map(|c| c.target.clone()).collect();
    assert_eq!(targets, vec!["room-door".to_owned(), "ROOM-LIGHT*".to_owned()]);
    let mut bare = p;
    bare.args.clear();
    assert!(doc.fix_problem(&bare).expect("fix"));
    assert!(doc.scene().entity(relay).expect("relay").entity.connections.is_empty());
}

#[test]
fn a_displacement_power_the_game_cannot_build_is_reported_and_fixed() {
    let mut doc = Document::new_empty();
    let id = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("solid");
    doc.create_displacements(&[FaceRef::new(id, 0)], 2).expect("displacement");
    assert!(find(&doc.check_problems(None), "disp_not_quad").is_none());
    doc.edit_solid(id, |s| {
        if let Some(d) = s.sides[0].disp.as_deref_mut() {
            d.power = 7;
        }
    })
    .expect("edit");
    let problems = doc.check_problems(None);
    let p = find(&problems, "disp_not_quad").expect("reported").clone();
    assert_eq!(p.faces, vec![FaceRef::new(id, 0)]);
    assert!(doc.fix_problem(&p).expect("fix"));
    assert!(!doc.scene().solid(id).expect("solid").has_disp(), "the broken displacement is removed: the brush is back");
}

#[test]
fn a_dispinfo_the_editor_cannot_read_is_reported_and_removed() {
    let mut map = Map::default();
    map.world.solids = vec![Solid { id: 2, sides: cube(DVec3::ZERO, 64.0), ..Solid::default() }];
    map.world.solids[0].sides[0].extra_blocks.push(hp_kv::Block::new("dispinfo"));
    let mut doc = Document::from_map(map);
    let p = find(&doc.check_problems(None), "disp_not_quad").expect("reported").clone();
    assert_eq!(p.faces, vec![FaceRef::new(2, 0)]);
    assert!(doc.fix_problem(&p).expect("fix"));
    assert!(doc.scene().solid(2).expect("solid").solid.sides[0].extra_blocks.is_empty());
}
