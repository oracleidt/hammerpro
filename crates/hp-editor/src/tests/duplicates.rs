use glam::DVec3;

use super::{box_sides, cube};
use crate::duplicates::{CODE_BRUSH, CODE_CONTAINED, CODE_ENTITY};
use crate::{Document, FaceRef, ObjRef, Owner, Problem, SelectOp};

fn find<'a>(problems: &'a [Problem], code: &str) -> Option<&'a Problem> {
    problems.iter().find(|p| p.code == code)
}

fn reordered(mut sides: Vec<hp_vmf::Side>, material: &str) -> Vec<hp_vmf::Side> {
    sides.reverse();
    for s in &mut sides {
        s.material = material.to_owned();
    }
    sides
}

#[test]
fn exact_copies_any_face_order_and_texture() {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("a");
    let b = doc.create_solid(reordered(cube(DVec3::ZERO, 64.0), "BRICK/BRICKWALL001A"), Owner::World).expect("b");
    let c = doc.create_solid(cube(DVec3::new(0.01, 0.0, -0.01), 64.0), Owner::World).expect("c");
    let other = doc.create_solid(cube(DVec3::new(0.5, 0.0, 0.0), 64.0), Owner::World).expect("other");
    let found = doc.find_redundant();
    assert_eq!(found.brushes, vec![(a, vec![b, c])]);
    assert!(!found.redundant_brushes().contains(&other));
    let problems = doc.check_problems(None);
    let p = find(&problems, CODE_BRUSH).expect("duplicate brush problem");
    assert_eq!(p.objects, vec![ObjRef::Solid(b), ObjRef::Solid(c)]);
    assert!(p.fixable);
    let n = doc.fix_problems(std::slice::from_ref(p)).expect("fix");
    assert_eq!(n, 1);
    assert!(doc.scene().solid(a).is_some() && doc.scene().solid(other).is_some());
    assert!(doc.scene().solid(b).is_none() && doc.scene().solid(c).is_none());
    assert!(find(&doc.check_problems(None), CODE_BRUSH).is_none());
    assert!(doc.undo());
    assert!(doc.scene().solid(b).is_some() && doc.scene().solid(c).is_some());
}

#[test]
fn copies_in_other_entities_are_not_brush_duplicates() {
    let mut doc = Document::new_empty();
    let world = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("world");
    let door = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("door");
    doc.tie_to_entity(&[ObjRef::Solid(door)], "func_door", &[]).expect("tie");
    let found = doc.find_redundant();
    assert!(found.brushes.is_empty(), "{found:?}");
    let d1 = doc.create_solid(cube(DVec3::new(256.0, 0.0, 0.0), 64.0), Owner::World).expect("d1");
    let d2 = doc.create_solid(cube(DVec3::new(256.0, 0.0, 0.0), 64.0), Owner::World).expect("d2");
    doc.tie_to_entity(&[ObjRef::Solid(d1)], "func_detail", &[]).expect("tie");
    doc.tie_to_entity(&[ObjRef::Solid(d2)], "func_detail", &[]).expect("tie");
    let found = doc.find_redundant();
    assert_eq!(found.brushes, vec![(d1, vec![d2])]);
    assert!(found.entities.is_empty());
    assert!(doc.scene().solid(world).is_some());
}

#[test]
fn duplicate_brush_entities_and_stacked_props() {
    let mut doc = Document::new_empty();
    let s1 = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("s1");
    let s2 = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("s2");
    let e1 = doc.tie_to_entity(&[ObjRef::Solid(s1)], "func_door", &[]).expect("tie");
    let e2 = doc.tie_to_entity(&[ObjRef::Solid(s2)], "func_door", &[]).expect("tie");
    let kv = |m: &str| vec![("model".to_owned(), m.to_owned()), ("angles".to_owned(), "0 90 0".to_owned())];
    let p1 = doc.create_point_entity("prop_static", DVec3::new(10.0, 20.0, 0.0), &kv("models/a.mdl")).expect("p1");
    let p2 = doc.create_point_entity("prop_static", DVec3::new(10.0, 20.0, 0.0), &kv("MODELS\\A.MDL")).expect("p2");
    let p3 = doc.create_point_entity("prop_static", DVec3::new(10.0, 20.0, 0.0), &kv("models/b.mdl")).expect("p3");
    let l1 = doc
        .create_point_entity("light", DVec3::new(0.0, 0.0, 100.0), &[("_light".to_owned(), "255 0 0 200".to_owned())])
        .expect("l1");
    let l2 = doc
        .create_point_entity("light", DVec3::new(0.0, 0.0, 100.0), &[("_light".to_owned(), "0 0 255 200".to_owned())])
        .expect("l2");
    let found = doc.find_redundant();
    let mut doors = [e1, e2];
    doors.sort_unstable();
    let mut props = [p1, p2];
    props.sort_unstable();
    assert!(found.entities.contains(&(doors[0], vec![doors[1]])), "{found:?}");
    assert!(found.entities.contains(&(props[0], vec![props[1]])), "{found:?}");
    assert!(!found.redundant_entities().contains(&p3));
    assert!(!found.redundant_entities().contains(&l1) && !found.redundant_entities().contains(&l2));
    let problems = doc.check_problems(None);
    let p = find(&problems, CODE_ENTITY).expect("entity problem");
    assert_eq!(p.objects.len(), 2);
    doc.fix_problems(std::slice::from_ref(p)).expect("fix");
    assert_eq!(doc.scene().entities().len(), 5);
    assert!(doc.find_redundant().entities.is_empty());
}

#[test]
fn brushes_inside_solid_brushes() {
    let mut doc = Document::new_empty();
    let outer = doc.create_solid(cube(DVec3::ZERO, 256.0), Owner::World).expect("outer");
    let inner = doc.create_solid(cube(DVec3::splat(64.0), 32.0), Owner::World).expect("inner");
    let flush = doc
        .create_solid(box_sides(DVec3::ZERO, DVec3::new(256.0, 64.0, 64.0), "TOOLS/TOOLSNODRAW"), Owner::World)
        .expect("flush");
    let clip = doc
        .create_solid(box_sides(DVec3::splat(100.0), DVec3::splat(120.0), "TOOLS/TOOLSPLAYERCLIP"), Owner::World)
        .expect("clip");
    let glass = doc
        .create_solid(box_sides(DVec3::splat(500.0), DVec3::splat(600.0), "TOOLS/TOOLSTRIGGER"), Owner::World)
        .expect("trigger");
    let in_trigger = doc.create_solid(cube(DVec3::splat(520.0), 16.0), Owner::World).expect("in trigger");
    let detail = doc.create_solid(cube(DVec3::splat(150.0), 16.0), Owner::World).expect("detail");
    doc.tie_to_entity(&[ObjRef::Solid(detail)], "func_detail", &[]).expect("tie");
    let found = doc.find_redundant();
    let inner_ids = found.contained_brushes();
    assert!(inner_ids.contains(&inner) && inner_ids.contains(&flush), "{found:?}");
    assert!(!inner_ids.contains(&clip) && !inner_ids.contains(&in_trigger) && !inner_ids.contains(&detail));
    assert!(!inner_ids.contains(&outer) && !inner_ids.contains(&glass));
    assert!(found.contained.contains(&(outer, inner)));
    let problems = doc.check_problems(None);
    let p = find(&problems, CODE_CONTAINED).expect("contained problem");
    doc.fix_problems(std::slice::from_ref(p)).expect("fix");
    assert!(doc.scene().solid(inner).is_none() && doc.scene().solid(flush).is_none());
    assert!(doc.scene().solid(outer).is_some() && doc.scene().solid(clip).is_some());
}

#[test]
fn displacement_copies_must_match_their_surface() {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("b");
    doc.create_displacements(&[FaceRef::new(a, 0), FaceRef::new(b, 0)], 2).expect("disp");
    assert_eq!(doc.find_redundant().brushes, vec![(a, vec![b])]);
    let brush = hp_geom::PaintBrush {
        center: DVec3::new(32.0, 32.0, 64.0),
        radius: 64.0,
        amount: 16.0,
        mode: hp_geom::PaintMode::Raise,
        falloff: hp_geom::Falloff::Constant,
        spatial: true,
    };
    doc.disp_paint(&[FaceRef::new(b, 0)], &brush).expect("paint");
    assert!(doc.find_redundant().brushes.is_empty());
    assert!(doc.find_redundant().contained.is_empty());
}

#[test]
fn live_check_of_pasted_copies() {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("a");
    let p = doc
        .create_point_entity(
            "prop_physics",
            DVec3::new(0.0, 0.0, 128.0),
            &[("model".to_owned(), "models/x.mdl".to_owned())],
        )
        .expect("prop");
    let copies = doc.duplicate(&[ObjRef::Solid(a), ObjRef::Entity(p)], DVec3::ZERO).expect("copy in place");
    let (_, created) = doc.last_step_created().expect("step");
    assert_eq!(created.len(), copies.len());
    let live = doc.duplicates_of(&created);
    assert_eq!(live.count(), 2, "{live:?}");
    assert_eq!(live.brushes[0].1, a);
    assert_eq!(live.entities[0].1, p);
    let moved = doc.duplicate(&[ObjRef::Solid(a)], DVec3::new(128.0, 0.0, 0.0)).expect("moved copy");
    assert!(doc.duplicates_of(&moved).is_empty());
    let solid = doc.scene().solid(a).expect("a").solid.clone();
    let remote = hp_vmf::Solid { id: 9000, ..solid };
    doc.apply_shared_update(crate::SharedUpdate {
        solids: vec![(9000, Some((remote, Owner::World)))],
        ..Default::default()
    })
    .expect("remote copy");
    let (_, created) = doc.last_step_created().expect("step");
    assert!(created.is_empty(), "copies made by Team Mapping peers are not reported");
}

#[test]
fn grouped_displacements_are_all_targets() {
    let mut doc = Document::new_empty();
    let ids: Vec<_> = (0..3)
        .map(|i| doc.create_solid(cube(DVec3::new(f64::from(i) * 64.0, 0.0, 0.0), 64.0), Owner::World).expect("solid"))
        .collect();
    let faces: Vec<FaceRef> = ids.iter().map(|id| FaceRef::new(*id, 0)).collect();
    doc.create_displacements(&faces, 2).expect("disp");
    doc.select_faces(&faces[..1], SelectOp::Replace);
    let objs: Vec<ObjRef> = ids.iter().map(|id| ObjRef::Solid(*id)).collect();
    doc.group(&objs).expect("group");
    let mut targets = doc.selected_disp_targets();
    targets.sort_unstable_by_key(|f| f.solid);
    assert_eq!(targets, faces, "the group selected after the face selection wins");
    doc.select_faces(&faces[1..2], SelectOp::Replace);
    assert_eq!(doc.selected_disp_targets(), vec![faces[1]]);
    let mut group = doc.group_disp_faces(ids[2]);
    group.sort_unstable_by_key(|f| f.solid);
    assert_eq!(group, faces);
    doc.clear_selection();
    doc.select_picked(ObjRef::Solid(ids[1]), crate::SelectMode::Groups, SelectOp::Replace);
    let mut targets = doc.selected_disp_targets();
    targets.sort_unstable_by_key(|f| f.solid);
    assert_eq!(targets, faces);
}

#[test]
#[ignore]
fn redundant_search_speed_on_a_big_map() {
    let Ok(path) = std::env::var("HP_DUP_BENCH") else { return };
    let doc = Document::open(std::path::Path::new(&path)).expect("open");
    let t0 = std::time::Instant::now();
    let snap = crate::limits::snapshot::MapSnapshot::capture(&doc);
    let capture = t0.elapsed();
    let t1 = std::time::Instant::now();
    let found = crate::limits::checks::redundant_in(&snap, &mut |m| {
        crate::limits::checks::MaterialFacts::guess(snap.material_key(m))
    });
    let search = t1.elapsed();
    let t2 = std::time::Instant::now();
    let problems = doc.check_problems(None);
    let checks = t2.elapsed();
    let t3 = std::time::Instant::now();
    let shapes: usize = doc.scene().solids().values().filter(|n| crate::duplicates::node_shape(n) != 0).count();
    let hashing = t3.elapsed();
    println!("{shapes} shapes hashed in {hashing:?}");
    println!(
        "{path}: {} solids, {} entities, snapshot {capture:?}, redundant search {search:?}, all checks {checks:?}",
        doc.scene().solids().len(),
        doc.scene().entities().len()
    );
    println!(
        "duplicate brushes {}, contained {}, duplicate entities {}, problems {}",
        found.redundant_brushes().len(),
        found.contained_brushes().len(),
        found.redundant_entities().len(),
        problems.len()
    );
}
