use glam::DVec3;
use hp_geom::Aabb;

use super::{cube, no_bounds};
use crate::{Document, FaceRef, ObjRef, Owner, RectMode, SelectMode, SelectOp};

fn scene() -> (Document, [crate::Id; 6]) {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 32.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::new(64.0, 0.0, 0.0), 32.0), Owner::World).expect("b");
    let door = doc
        .create_brush_entity(
            "func_door",
            &[("targetname".into(), "door".into())],
            vec![cube(DVec3::new(0.0, 64.0, 0.0), 32.0), cube(DVec3::new(32.0, 64.0, 0.0), 32.0)],
        )
        .expect("door");
    doc.group(&[ObjRef::Solid(a), ObjRef::Solid(b), ObjRef::Entity(door)]).expect("group");
    let lone = doc.create_solid(cube(DVec3::new(500.0, 0.0, 0.0), 32.0), Owner::World).expect("lone");
    let light = doc.create_point_entity("light", DVec3::new(0.0, 0.0, 200.0), &[]).expect("light");
    let door_solid = doc.scene().entity_solid_ids(door)[0];
    doc.clear_selection();
    (doc, [a, b, door, lone, light, door_solid])
}

#[test]
fn modes() {
    let (doc, [a, b, door, lone, light, door_solid]) = scene();
    let groups = doc.resolve(ObjRef::Solid(door_solid), SelectMode::Groups);
    assert_eq!(groups, vec![ObjRef::Solid(a), ObjRef::Solid(b), ObjRef::Entity(door)]);
    assert_eq!(doc.resolve(ObjRef::Solid(door_solid), SelectMode::Objects), vec![ObjRef::Entity(door)]);
    assert_eq!(doc.resolve(ObjRef::Solid(door_solid), SelectMode::Solids), vec![ObjRef::Solid(door_solid)]);
    assert_eq!(doc.resolve(ObjRef::Solid(a), SelectMode::Objects), vec![ObjRef::Solid(a)]);
    assert_eq!(doc.resolve(ObjRef::Solid(lone), SelectMode::Groups), vec![ObjRef::Solid(lone)]);
    assert_eq!(doc.resolve(ObjRef::Entity(light), SelectMode::Groups), vec![ObjRef::Entity(light)]);
    assert!(doc.resolve(ObjRef::Solid(9999), SelectMode::Groups).is_empty());
    let many = doc.resolve_many(&[ObjRef::Solid(a), ObjRef::Solid(door_solid)], SelectMode::Groups);
    assert_eq!(many.len(), 3);
}

#[test]
fn select_operations() {
    let (mut doc, [a, b, door, lone, light, door_solid]) = scene();
    doc.select_picked(ObjRef::Solid(door_solid), SelectMode::Objects, SelectOp::Replace);
    assert!(doc.is_solid_selected(door_solid));
    assert!(doc.is_selected(ObjRef::Solid(door_solid)));
    assert_eq!(doc.selected_solid_ids().len(), 2);
    assert_eq!(doc.selected_entity_ids(), vec![door]);
    doc.select(&[ObjRef::Solid(a)], SelectOp::Add);
    doc.toggle_selection(&[ObjRef::Solid(a), ObjRef::Solid(b)]);
    assert_eq!(doc.selected_objects(), vec![ObjRef::Entity(door), ObjRef::Solid(b)]);
    doc.deselect(&[ObjRef::Entity(door)]);
    assert_eq!(doc.selected_objects(), vec![ObjRef::Solid(b)]);
    doc.set_selection(&[ObjRef::Solid(lone), ObjRef::Solid(424242)]);
    assert_eq!(doc.selected_objects(), vec![ObjRef::Solid(lone)]);
    doc.select_all();
    assert_eq!(doc.selection().objects.len(), 5);
    doc.set_selection(&[ObjRef::Solid(a), ObjRef::Entity(light)]);
    doc.invert_selection();
    let inv = doc.selected_objects();
    assert_eq!(inv.len(), 3);
    assert!(
        inv.contains(&ObjRef::Solid(b)) && inv.contains(&ObjRef::Entity(door)) && inv.contains(&ObjRef::Solid(lone))
    );
    doc.clear_selection();
    assert!(doc.selection().is_empty());
    doc.select_faces(&[FaceRef::new(a, 0), FaceRef::new(a, 1), FaceRef::new(a, 99)], SelectOp::Replace);
    assert_eq!(doc.selected_faces().len(), 2);
    doc.select_faces(&[FaceRef::new(a, 0)], SelectOp::Toggle);
    assert_eq!(doc.selected_faces(), vec![FaceRef::new(a, 1)]);
    let fb = doc.selection_bounds(&no_bounds).expect("face bounds");
    assert_eq!(fb.max.z, 0.0);
    doc.clear_face_selection();
    assert!(doc.selection().faces.is_empty());
}

#[test]
fn queries() {
    let (mut doc, [a, b, door, lone, light, _]) = scene();
    assert_eq!(doc.select_by_class("func_*", SelectOp::Replace), 1);
    assert_eq!(doc.selected_objects(), vec![ObjRef::Entity(door)]);
    assert_eq!(doc.select_by_targetname("DOOR", SelectOp::Replace), 1);
    assert_eq!(doc.select_by_class("light", SelectOp::Add), 1);
    assert_eq!(doc.selection().objects.len(), 2);
    doc.apply_material(&[FaceRef::new(lone, 2)], "TOOLS/TOOLSSKIP").expect("material");
    assert_eq!(doc.select_by_material("tools/*", SelectOp::Replace), 1);
    assert_eq!(doc.selected_objects(), vec![ObjRef::Solid(lone)]);
    assert_eq!(doc.select_faces_by_material("tools/toolsskip", SelectOp::Replace), 1);
    let n = doc.select_in_box(
        &Aabb::new(DVec3::splat(-1.0), DVec3::new(70.0, 40.0, 40.0)),
        RectMode::Touching,
        &no_bounds,
        SelectOp::Replace,
    );
    assert_eq!(n, 2);
    assert_eq!(doc.selected_objects(), vec![ObjRef::Solid(a), ObjRef::Solid(b)]);
    let n = doc.select_in_box(
        &Aabb::new(DVec3::splat(-1.0), DVec3::new(70.0, 40.0, 40.0)),
        RectMode::Inside,
        &no_bounds,
        SelectOp::Replace,
    );
    assert_eq!(n, 1);
    doc.set_selection(&[ObjRef::Solid(a), ObjRef::Entity(light)]);
    let sb = doc.selection_bounds(&no_bounds).expect("bounds");
    assert_eq!(sb.min, DVec3::new(-8.0, -8.0, 0.0));
    assert_eq!(sb.max, DVec3::new(32.0, 32.0, 208.0));
    let big = |_: &hp_vmf::Entity| Some(Aabb::new(DVec3::splat(-100.0), DVec3::splat(100.0)));
    assert_eq!(doc.selection_center(&big), Some(DVec3::ZERO));
}

#[test]
fn face_selection_back_and_forward() {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 32.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::new(64.0, 0.0, 0.0), 32.0), Owner::World).expect("b");
    let f = |s, i| FaceRef::new(s, i);
    assert!(!doc.can_face_selection_back());
    doc.select_faces(&[f(a, 0)], SelectOp::Replace);
    doc.select_faces(&[f(a, 1)], SelectOp::Add);
    doc.select_faces(&[f(b, 2)], SelectOp::Replace);
    assert_eq!(doc.selected_faces(), vec![f(b, 2)]);

    assert!(doc.face_selection_back());
    assert_eq!(doc.selected_faces(), vec![f(a, 0), f(a, 1)], "one step back");
    assert!(doc.face_selection_back());
    assert_eq!(doc.selected_faces(), vec![f(a, 0)]);
    assert!(doc.face_selection_back());
    assert!(doc.selected_faces().is_empty(), "back to no face at all");
    assert!(!doc.face_selection_back(), "nothing older");

    assert!(doc.face_selection_forward());
    assert_eq!(doc.selected_faces(), vec![f(a, 0)]);
    assert!(doc.can_face_selection_forward());
    doc.select_faces(&[f(b, 0)], SelectOp::Replace);
    assert!(!doc.can_face_selection_forward());
    assert!(doc.face_selection_back());
    assert_eq!(doc.selected_faces(), vec![f(a, 0)]);

    doc.select_faces(&[f(a, 3), f(b, 3)], SelectOp::Replace);
    doc.clear_face_selection();
    doc.delete(&[ObjRef::Solid(b)]).expect("delete");
    assert!(doc.face_selection_back());
    assert_eq!(doc.selected_faces(), vec![f(a, 3)], "the deleted solid's face is left out");
}

#[test]
fn select_filter_limits_box_selection() {
    use crate::{FilterKind, RectMode, SelectFilter};
    let mut doc = Document::new_empty();
    let w = doc.create_solid(super::cube(glam::DVec3::ZERO, 16.0), Owner::World).expect("w");
    let l1 = doc.create_point_entity("light", glam::DVec3::new(40.0, 0.0, 0.0), &[]).expect("l1");
    let l2 = doc
        .create_point_entity(
            "light_spot",
            glam::DVec3::new(80.0, 0.0, 0.0),
            &[("targetname".into(), "door_lamp".into())],
        )
        .expect("l2");
    let pr = doc
        .create_point_entity(
            "prop_static",
            glam::DVec3::new(120.0, 0.0, 0.0),
            &[("model".into(), "models/props/chair.mdl".into())],
        )
        .expect("pr");
    let d = doc
        .create_brush_entity(
            "func_door",
            &[("targetname".into(), "door1".into())],
            vec![super::cube(glam::DVec3::new(160.0, 0.0, 0.0), 16.0)],
        )
        .expect("d");
    let eb = |e: &hp_vmf::Entity| {
        let o = e.origin()?;
        Some(hp_geom::Aabb::new(o - glam::DVec3::splat(8.0), o + glam::DVec3::splat(8.0)))
    };
    let all = hp_geom::Aabb::new(glam::DVec3::splat(-500.0), glam::DVec3::splat(500.0));
    let run = |doc: &mut Document, f: &SelectFilter| {
        doc.select_in_box_filtered(&all, RectMode::Touching, &eb, crate::SelectOp::Replace, Some(f));
        let mut v = doc.selected_objects();
        v.sort_by_key(|o| match o {
            ObjRef::Solid(i) | ObjRef::Entity(i) => *i,
        });
        v
    };
    assert_eq!(run(&mut doc, &SelectFilter::new("class:light")), vec![ObjRef::Entity(l1)]);
    assert_eq!(run(&mut doc, &SelectFilter::new("class:light*")), vec![ObjRef::Entity(l1), ObjRef::Entity(l2)]);
    assert_eq!(run(&mut doc, &SelectFilter::new("name:door*")), vec![ObjRef::Entity(l2), ObjRef::Entity(d)]);
    assert_eq!(run(&mut doc, &SelectFilter::new("name:door* class:func_*")), vec![ObjRef::Entity(d)]);
    assert_eq!(
        run(&mut doc, &SelectFilter::new("class:light,prop_static")),
        vec![ObjRef::Entity(l1), ObjRef::Entity(pr)]
    );
    assert_eq!(run(&mut doc, &SelectFilter::new("model:chair")), vec![ObjRef::Entity(pr)]);
    assert_eq!(run(&mut doc, &SelectFilter::new("material:TOOLS/*")).len(), 0);
    assert_eq!(run(&mut doc, &SelectFilter::new("material:dev/*")), vec![ObjRef::Solid(w), ObjRef::Entity(d)]);
    let mut kinds = SelectFilter::default();
    assert!(!kinds.is_active());
    kinds.set_kinds(&[FilterKind::Brushes]);
    assert!(kinds.is_active());
    assert_eq!(run(&mut doc, &kinds), vec![ObjRef::Solid(w)]);
    kinds.set_kinds(&[FilterKind::Lights, FilterKind::Props]);
    assert_eq!(run(&mut doc, &kinds), vec![ObjRef::Entity(l1), ObjRef::Entity(l2), ObjRef::Entity(pr)]);
    let neg = SelectFilter::new("-class:light*");
    assert_eq!(run(&mut doc, &neg).len(), 3);
    assert_eq!(run(&mut doc, &SelectFilter::default()).len(), 5);
    assert_eq!(SelectFilter::new("class:light").summary(), "class:light");
}
