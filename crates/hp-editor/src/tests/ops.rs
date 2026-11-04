use glam::{DMat4, DQuat, DVec2, DVec3};
use hp_geom::{Aabb, Falloff, Justify, PaintBrush, PaintMode, Plane, Primitive, PrimitiveParams, TextureAlign};
use hp_vmf::Connection;

use super::{bounds, box_sides, check_undo_redo, cube, doc_with_cube, no_bounds, vmf};
use crate::{
    AlignMode, ClipKeep, Document, EditOptions, EntityQuery, FaceEdit, FaceRef, ObjRef, Owner, ReplaceScope, SelectOp,
};

fn approx(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-6
}

#[test]
fn create_solids_and_entities() {
    let mut doc = Document::new_empty();
    let id = check_undo_redo(&mut doc, |d| d.create_solid(cube(DVec3::ZERO, 32.0), Owner::World).expect("create"));
    let n = doc.scene().solid(id).expect("solid");
    assert!(n.geom.valid);
    assert_eq!(n.solid.sides.len(), 6);
    let mut side_ids: Vec<u32> = n.solid.sides.iter().map(|s| s.id).collect();
    side_ids.dedup();
    assert_eq!(side_ids.len(), 6);
    assert!(doc.selection().contains(ObjRef::Solid(id)));
    let mut bad = cube(DVec3::ZERO, 32.0);
    bad.truncate(3);
    assert!(doc.create_solid(bad, Owner::World).is_err());
    assert!(doc.create_solid(cube(DVec3::ZERO, 8.0), Owner::Entity(999)).is_err());
    let kv = vec![("targetname".to_owned(), "spot".to_owned()), ("classname".to_owned(), "ignored".to_owned())];
    let e = check_undo_redo(&mut doc, |d| {
        d.create_point_entity("info_target", DVec3::new(1.0, 2.0, 3.0), &kv).expect("entity")
    });
    let ent = &doc.scene().entity(e).expect("entity").entity;
    assert_eq!(ent.classname, "info_target");
    assert_eq!(ent.get("origin"), Some("1 2 3"));
    assert_eq!(ent.get("targetname"), Some("spot"));
    assert_eq!(ent.props.last().map(|p| p.0.as_str()), Some("origin"));
    assert!(doc.create_point_entity("", DVec3::ZERO, &[]).is_err());
    assert!(doc.create_point_entity("x", DVec3::NAN, &[]).is_err());
    let b = check_undo_redo(&mut doc, |d| {
        d.create_brush_entity("func_detail", &[], vec![cube(DVec3::new(100.0, 0.0, 0.0), 16.0)]).expect("brush")
    });
    assert!(doc.scene().is_brush_entity(b));
    assert!(id != e && e != b);
}

#[test]
fn primitives() {
    let mut doc = Document::new_empty();
    let params = PrimitiveParams {
        kind: Primitive::Arch,
        bounds: Aabb::new(DVec3::ZERO, DVec3::new(256.0, 256.0, 64.0)),
        sides: 8,
        ..PrimitiveParams::default()
    };
    let ids = check_undo_redo(&mut doc, |d| d.create_primitive(&params, Owner::World).expect("arch"));
    assert!(ids.len() > 1);
    assert_eq!(doc.scene().groups().len(), 1, "multi solid primitives are grouped");
    let cyl = PrimitiveParams { kind: Primitive::Cylinder, ..PrimitiveParams::default() };
    let ids = doc.create_primitive(&cyl, Owner::World).expect("cylinder");
    assert_eq!(ids.len(), 1);
    assert_eq!(doc.scene().groups().len(), 1);
}

#[test]
fn delete_cascades() {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 32.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::new(64.0, 0.0, 0.0), 32.0), Owner::World).expect("b");
    let g = doc.group(&[ObjRef::Solid(a), ObjRef::Solid(b)]).expect("group");
    let e = doc.create_brush_entity("func_door", &[], vec![cube(DVec3::new(0.0, 64.0, 0.0), 16.0)]).expect("door");
    let door_solid = doc.scene().entity_solid_ids(e)[0];
    let n = check_undo_redo(&mut doc, |d| d.delete(&[ObjRef::Solid(door_solid)]).expect("delete"));
    assert_eq!(n, 2);
    assert!(doc.scene().entity(e).is_none());
    check_undo_redo(&mut doc, |d| d.delete(&[ObjRef::Solid(a), ObjRef::Solid(b)]).expect("delete"));
    assert!(doc.scene().group(g).is_none());
    assert_eq!(doc.delete(&[ObjRef::Solid(12345)]).expect("nothing"), 0);
}

#[test]
fn duplicate_with_groups() {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 32.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::new(64.0, 0.0, 0.0), 32.0), Owner::World).expect("b");
    let e = doc.create_brush_entity("func_door", &[], vec![cube(DVec3::new(0.0, 64.0, 0.0), 16.0)]).expect("door");
    doc.group(&[ObjRef::Solid(a), ObjRef::Solid(b), ObjRef::Entity(e)]).expect("group");
    let objs = doc.resolve(ObjRef::Solid(a), crate::SelectMode::Groups);
    assert_eq!(objs.len(), 3);
    let copies = check_undo_redo(&mut doc, |d| d.duplicate(&objs, DVec3::new(0.0, 0.0, 128.0)).expect("dup"));
    assert_eq!(copies.len(), 3);
    assert_eq!(doc.scene().groups().len(), 2, "the group is duplicated");
    assert_eq!(doc.scene().solids().len(), 6);
    let copy_solid = copies.iter().find(|o| o.is_solid()).copied().expect("solid copy");
    assert_eq!(bounds(&doc, copy_solid).min.z, 128.0);
    let resolved = doc.resolve(copy_solid, crate::SelectMode::Groups);
    assert_eq!(resolved.len(), 3);
    assert!(resolved.iter().all(|o| copies.contains(o)));
    doc.duplicate(&[ObjRef::Solid(a)], DVec3::X * 256.0).expect("dup one");
    assert_eq!(doc.scene().groups().len(), 2);
    assert!(doc.duplicate(&[], DVec3::ZERO).is_err());
}

#[test]
fn translate_with_texture_lock() {
    let (mut doc, id) = doc_with_cube();
    let side0 = doc.scene().solid(id).expect("s").solid.sides[0].clone();
    check_undo_redo(&mut doc, |d| d.translate(&[ObjRef::Solid(id)], DVec3::new(16.0, 0.0, 0.0)).expect("move"));
    let moved = doc.scene().solid(id).expect("s").clone();
    assert_eq!(moved.bounds.min, DVec3::new(16.0, 0.0, 0.0));
    let p = DVec3::new(10.0, 10.0, 64.0);
    let uv0 = hp_geom::texel_uv(&side0, p);
    let uv1 = hp_geom::texel_uv(&moved.solid.sides[0], p + DVec3::new(16.0, 0.0, 0.0));
    assert!((uv0 - uv1).length() < 1e-9);
    doc.options_mut().texture_lock = false;
    doc.translate(&[ObjRef::Solid(id)], DVec3::new(16.0, 0.0, 0.0)).expect("move");
    let s = &doc.scene().solid(id).expect("s").solid.sides[0];
    assert_eq!(s.uaxis.shift, moved.solid.sides[0].uaxis.shift);
    assert!(doc.translate(&[ObjRef::Solid(id)], DVec3::NAN).is_err());
    let before = vmf(&doc);
    doc.translate(&[ObjRef::Solid(id)], DVec3::ZERO).expect("zero move");
    assert_eq!(vmf(&doc), before);
    assert_eq!(*doc.options(), EditOptions { texture_lock: false, ..EditOptions::default() });
}

#[test]
fn rotate_scale_flip_transform() {
    let mut doc = Document::new_empty();
    let s = doc.create_solid(box_sides(DVec3::ZERO, DVec3::new(64.0, 32.0, 16.0), "A/B"), Owner::World).expect("s");
    let e = doc
        .create_point_entity("info_player_start", DVec3::new(100.0, 0.0, 0.0), &[("angles".into(), "0 0 0".into())])
        .expect("e");
    let objs = [ObjRef::Solid(s), ObjRef::Entity(e)];
    check_undo_redo(&mut doc, |d| d.rotate_axis(&objs, DVec3::ZERO, DVec3::Z, 90.0).expect("rotate"));
    let b = bounds(&doc, ObjRef::Solid(s));
    assert!(approx(b.min, DVec3::new(-32.0, 0.0, 0.0)) && approx(b.max, DVec3::new(0.0, 64.0, 16.0)), "{b:?}");
    let ent = &doc.scene().entity(e).expect("e").entity;
    assert_eq!(ent.get("origin"), Some("0 100 0"));
    assert_eq!(ent.get("angles"), Some("0 90 0"));
    doc.rotate(&objs, DVec3::ZERO, DQuat::from_rotation_z(-90f64.to_radians())).expect("rotate back");
    assert!(approx(bounds(&doc, ObjRef::Solid(s)).max, DVec3::new(64.0, 32.0, 16.0)));
    assert!(doc.rotate(&objs, DVec3::ZERO, DQuat::from_xyzw(0.0, 0.0, 0.0, 0.0)).is_err());
    let from = bounds(&doc, ObjRef::Solid(s));
    let to = Aabb::new(DVec3::ZERO, DVec3::new(128.0, 32.0, 32.0));
    check_undo_redo(&mut doc, |d| d.scale_bounds(&[ObjRef::Solid(s)], &from, &to).expect("scale"));
    assert!(approx(bounds(&doc, ObjRef::Solid(s)).max, to.max));
    assert!(doc.scene().solid(s).expect("s").geom.valid);
    check_undo_redo(&mut doc, |d| d.flip(&[ObjRef::Solid(s)], 0, DVec3::ZERO).expect("flip"));
    let n = doc.scene().solid(s).expect("s");
    assert!(n.geom.valid);
    assert!(approx(n.bounds.min, DVec3::new(-128.0, 0.0, 0.0)));
    assert!(doc.flip(&[ObjRef::Solid(s)], 3, DVec3::ZERO).is_err());
    let m = DMat4::from_translation(DVec3::new(0.0, 0.0, 64.0));
    check_undo_redo(&mut doc, |d| d.transform(&[ObjRef::Solid(s)], m).expect("transform"));
    assert_eq!(bounds(&doc, ObjRef::Solid(s)).min.z, 64.0);
    assert!(doc.transform(&[ObjRef::Solid(s)], DMat4::ZERO).is_err());
}

#[test]
fn snap_and_align() {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::new(3.0, 5.0, 0.0), 16.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::new(41.0, 70.0, 0.0), 16.0), Owner::World).expect("b");
    check_undo_redo(&mut doc, |d| {
        d.snap_to_grid(&[ObjRef::Solid(a), ObjRef::Solid(b)], 8.0, true, &no_bounds).expect("snap")
    });
    assert_eq!(bounds(&doc, ObjRef::Solid(a)).min, DVec3::new(0.0, 8.0, 0.0));
    assert_eq!(bounds(&doc, ObjRef::Solid(b)).min, DVec3::new(40.0, 72.0, 0.0));
    assert!(doc.snap_to_grid(&[ObjRef::Solid(a)], 0.0, true, &no_bounds).is_err());
    let reference = bounds(&doc, ObjRef::Solid(b));
    check_undo_redo(&mut doc, |d| {
        d.align_objects(&[ObjRef::Solid(a)], &reference, 1, AlignMode::Max, true, &no_bounds).expect("align")
    });
    assert_eq!(bounds(&doc, ObjRef::Solid(a)).max.y, reference.max.y);
    doc.align_objects(&[ObjRef::Solid(a)], &reference, 0, AlignMode::Center, false, &no_bounds).expect("center");
    assert_eq!(bounds(&doc, ObjRef::Solid(a)).center().x, reference.center().x);
    doc.align_objects(&[ObjRef::Solid(a)], &reference, 2, AlignMode::Min, false, &no_bounds).expect("min");
    assert!(doc.align_objects(&[], &Aabb::EMPTY, 0, AlignMode::Min, false, &no_bounds).is_err());
}

#[test]
fn tie_and_move_to_world() {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 32.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::new(64.0, 0.0, 0.0), 32.0), Owner::World).expect("b");
    let kv = vec![("speed".to_owned(), "100".to_owned())];
    let e = check_undo_redo(&mut doc, |d| {
        d.tie_to_entity(&[ObjRef::Solid(a), ObjRef::Solid(b)], "func_door", &kv).expect("tie")
    });
    assert_eq!(doc.scene().entity_solid_ids(e), vec![a, b]);
    assert!(doc.selection().contains(ObjRef::Entity(e)));
    assert_eq!(doc.scene().entity(e).expect("e").entity.get("speed"), Some("100"));
    let e2 = doc.tie_to_entity(&[ObjRef::Entity(e)], "func_wall", &[]).expect("retie");
    assert!(doc.scene().entity(e).is_none());
    assert_eq!(doc.scene().entity_solid_ids(e2).len(), 2);
    let n = check_undo_redo(&mut doc, |d| d.move_to_world(&[ObjRef::Entity(e2)]).expect("to world"));
    assert_eq!(n, 2);
    assert!(doc.scene().entity(e2).is_none());
    assert_eq!(doc.scene().solid(a).expect("a").owner, Owner::World);
    assert!(doc.tie_to_entity(&[], "func_door", &[]).is_err());
    assert!(doc.tie_to_entity(&[ObjRef::Solid(a)], " ", &[]).is_err());
    assert_eq!(doc.move_to_world(&[ObjRef::Solid(a)]).expect("noop"), 0);
}

#[test]
fn materials() {
    let (mut doc, id) = doc_with_cube();
    let faces: Vec<FaceRef> = (0..6).map(|i| FaceRef::new(id, i)).collect();
    let n = check_undo_redo(&mut doc, |d| d.apply_material(&faces[..2], "BRICK/BRICKWALL001A").expect("apply"));
    assert_eq!(n, 2);
    assert_eq!(doc.face_side(faces[0]).expect("side").material, "BRICK/BRICKWALL001A");
    let geom_before = doc.scene().solid(id).expect("s").geom.clone();
    doc.apply_material(&faces[2..3], "CONCRETE/X").expect("apply");
    assert!(std::sync::Arc::ptr_eq(&geom_before, &doc.scene().solid(id).expect("s").geom));
    assert!(doc.apply_material(&[FaceRef::new(id, 99)], "X").is_err());
    assert!(doc.apply_material(&faces, "  ").is_err());
    let edit = FaceEdit {
        u_scale: Some(0.5),
        v_scale: Some(0.5),
        u_shift: Some(16.0),
        rotation: Some(45.0),
        lightmap_scale: Some(32),
        smoothing_groups: Some(3),
        ..FaceEdit::default()
    };
    check_undo_redo(&mut doc, |d| d.edit_faces(&faces, &edit).expect("edit"));
    let s = doc.face_side(faces[0]).expect("side");
    assert_eq!((s.uaxis.scale, s.vaxis.scale, s.uaxis.shift), (0.5, 0.5, 16.0));
    assert_eq!((s.rotation, s.lightmap_scale, s.smoothing_groups), (45.0, 32, 3));
    assert!(doc.edit_faces(&faces, &FaceEdit { lightmap_scale: Some(0), ..FaceEdit::default() }).is_err());
    assert!(doc.edit_faces(&faces, &FaceEdit { u_axis: Some(DVec3::ZERO), ..FaceEdit::default() }).is_err());
    let reset =
        FaceEdit { align: Some(TextureAlign::World), shift_by: Some(DVec2::new(1.0, 2.0)), ..FaceEdit::default() };
    doc.edit_faces(&faces, &reset).expect("reset");
    let s = doc.face_side(faces[0]).expect("side");
    assert_eq!((s.rotation, s.uaxis.shift, s.vaxis.shift), (0.0, 1.0, 2.0));
    let size = |_: &str| Some((128u32, 128u32));
    check_undo_redo(&mut doc, |d| d.justify_faces(&faces[..1], Justify::Left, false, &size).expect("justify"));
    doc.fit_faces(&faces[..1], 2.0, 1.0, false, &size).expect("fit");
    let s = doc.face_side(faces[0]).expect("side");
    assert!((s.uaxis.scale - 0.25).abs() < 1e-9);
    doc.justify_faces(&faces, Justify::Center, true, &|_| None).expect("treat as one");
    doc.align_faces_to_view(&faces[..1], DVec3::X, DVec3::Y).expect("view");
    let n =
        check_undo_redo(&mut doc, |d| d.replace_materials("brick/*", "stone/*", &ReplaceScope::All).expect("replace"));
    assert_eq!(n, 2);
    assert_eq!(doc.face_side(faces[0]).expect("side").material, "stone/brickwall001a");
    assert_eq!(doc.replace_materials("stone/*", "STONE/*", &ReplaceScope::All).expect("same"), 0);
    assert_eq!(doc.replace_materials("nothing/*", "x", &ReplaceScope::All).expect("none"), 0);
    doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    let n = doc.replace_materials("*", "dev/dev_blendmeasure", &ReplaceScope::Selection).expect("all");
    assert_eq!(n, 6);
    assert_eq!(doc.used_materials(), vec![("dev/dev_blendmeasure".to_owned(), 6)]);
    let n = doc.apply_material_to_objects(&[ObjRef::Solid(id)], "TOOLS/TOOLSNODRAW").expect("objects");
    assert_eq!(n, 6);
    assert!(doc.replace_materials("", "x", &ReplaceScope::All).is_err());
}

#[test]
fn wrap_texture_across_edge() {
    let (mut doc, id) = doc_with_cube();
    let top = FaceRef::new(id, 0);
    let wall = FaceRef::new(id, 4);
    doc.edit_faces(&[top], &FaceEdit { u_shift: Some(7.0), ..FaceEdit::default() }).expect("shift");
    check_undo_redo(&mut doc, |d| d.wrap_texture(top, &[wall]).expect("wrap"));
    let src = doc.face_side(top).expect("src").clone();
    let dst = doc.face_side(wall).expect("dst").clone();
    assert_eq!(src.material, dst.material);
    let p = DVec3::new(64.0, 20.0, 64.0);
    let a = hp_geom::texel_uv(&src, p);
    let b = hp_geom::texel_uv(&dst, p);
    assert!((a - b).length() < 1e-6, "{a:?} {b:?}");
}

#[test]
fn clip_modes() {
    let plane = Plane::from_normal_point(DVec3::X, DVec3::new(16.0, 0.0, 0.0)).expect("plane");
    for keep in [ClipKeep::Both, ClipKeep::Front, ClipKeep::Back] {
        let (mut doc, id) = doc_with_cube();
        doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
        let ids = check_undo_redo(&mut doc, |d| d.clip(&[id], &plane, keep).expect("clip"));
        let total: f64 = ids.iter().map(|i| doc.scene().solid(*i).expect("piece").geom.volume()).sum();
        match keep {
            ClipKeep::Both => {
                assert_eq!(ids.len(), 2);
                assert!((total - 64.0f64.powi(3)).abs() < 1e-6);
                assert_eq!(doc.selection().objects.len(), 2, "both pieces selected");
            }
            ClipKeep::Front => assert!((total - 48.0 * 64.0 * 64.0).abs() < 1e-6),
            ClipKeep::Back => assert!((total - 16.0 * 64.0 * 64.0).abs() < 1e-6),
        }
    }
    let (mut doc, id) = doc_with_cube();
    let far = Plane::from_normal_point(DVec3::X, DVec3::new(500.0, 0.0, 0.0)).expect("plane");
    assert!(doc.clip(&[id], &far, ClipKeep::Both).expect("clip").is_empty());
    assert!(doc.clip(&[id], &Plane { normal: DVec3::ZERO, dist: 0.0 }, ClipKeep::Both).is_err());
}

#[test]
fn carve_world_solids() {
    let mut doc = Document::new_empty();
    let wall =
        doc.create_solid(box_sides(DVec3::ZERO, DVec3::new(256.0, 16.0, 128.0), "W/W"), Owner::World).expect("w");
    let door = doc
        .create_solid(box_sides(DVec3::new(96.0, -8.0, 0.0), DVec3::new(160.0, 24.0, 96.0), "D/D"), Owner::World)
        .expect("d");
    let untouched = doc.create_solid(cube(DVec3::new(1000.0, 0.0, 0.0), 16.0), Owner::World).expect("u");
    let n = check_undo_redo(&mut doc, |d| d.carve(&[door]).expect("carve"));
    assert_eq!(n, 1);
    assert!(doc.scene().solid(wall).is_none(), "carved solid is replaced by pieces");
    assert!(doc.scene().solid(door).is_some());
    assert!(doc.scene().solid(untouched).is_some());
    let volume: f64 = doc
        .scene()
        .solids()
        .values()
        .filter(|n| n.solid.id != door && n.solid.id != untouched)
        .map(|n| n.geom.volume())
        .sum();
    assert!((volume - (256.0 * 16.0 * 128.0 - 64.0 * 16.0 * 96.0)).abs() < 1e-6);
    assert!(doc.carve(&[]).is_err());
}

#[test]
fn hollow_and_merge() {
    let (mut doc, id) = doc_with_cube();
    let walls = check_undo_redo(&mut doc, |d| d.hollow(&[id], 8.0).expect("hollow"));
    assert_eq!(walls.len(), 6);
    assert_eq!(doc.scene().groups().len(), 1, "walls are grouped");
    let vol: f64 = walls.iter().map(|w| doc.scene().solid(*w).expect("wall").geom.volume()).sum();
    assert!((vol - (64.0f64.powi(3) - 48.0f64.powi(3))).abs() < 1e-6);
    assert!(doc.hollow(&walls[..1], 100.0).is_err());
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 32.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::new(32.0, 0.0, 0.0), 32.0), Owner::World).expect("b");
    let m = check_undo_redo(&mut doc, |d| d.merge_solids(&[a, b]).expect("merge"));
    assert_eq!(m, a);
    assert!(doc.scene().solid(b).is_none());
    let n = doc.scene().solid(a).expect("merged");
    assert_eq!(n.bounds.max, DVec3::new(64.0, 32.0, 32.0));
    assert_eq!(n.solid.sides.len(), 6);
    assert!(doc.merge_solids(&[a]).is_err());
}

#[test]
fn vertex_editing() {
    let (mut doc, id) = doc_with_cube();
    let (mut pts, faces) = doc.solid_vertices(id).expect("vertices");
    assert_eq!(pts.len(), 8);
    assert_eq!(faces.len(), 6);
    let i = pts.iter().position(|p| *p == DVec3::new(64.0, 64.0, 64.0)).expect("corner");
    pts[i].z = 96.0;
    check_undo_redo(&mut doc, |d| d.set_solid_vertices(id, &pts).expect("edit"));
    let n = doc.scene().solid(id).expect("solid");
    assert!(n.geom.valid);
    assert_eq!(n.bounds.max.z, 96.0);
    let before = vmf(&doc);
    let flat: Vec<DVec3> = pts.iter().map(|p| DVec3::new(p.x, p.y, 0.0)).collect();
    assert!(doc.set_solid_vertices(id, &flat).is_err());
    assert!(doc.set_solid_vertices(id, &[DVec3::NAN; 4]).is_err());
    assert_eq!(vmf(&doc), before);
}

#[test]
fn groups() {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 16.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::new(32.0, 0.0, 0.0), 16.0), Owner::World).expect("b");
    let c = doc.create_solid(cube(DVec3::new(64.0, 0.0, 0.0), 16.0), Owner::World).expect("c");
    let g1 = check_undo_redo(&mut doc, |d| d.group(&[ObjRef::Solid(a), ObjRef::Solid(b)]).expect("group"));
    assert_eq!(doc.scene().object_group(ObjRef::Solid(a)), Some(g1));
    let g2 =
        check_undo_redo(&mut doc, |d| d.group(&[ObjRef::Solid(a), ObjRef::Solid(b), ObjRef::Solid(c)]).expect("nest"));
    assert_eq!(doc.scene().group_parent(g1), Some(g2));
    assert_eq!(doc.scene().group_root(g1), g2);
    assert_eq!(doc.scene().group_members(g2).len(), 3);
    let n = check_undo_redo(&mut doc, |d| d.ungroup(&[ObjRef::Solid(a)]).expect("ungroup"));
    assert_eq!(n, 1);
    assert!(doc.scene().group(g2).is_none());
    assert_eq!(doc.scene().group_parent(g1), None);
    assert_eq!(doc.scene().object_group(ObjRef::Solid(c)), None);
    assert_eq!(doc.scene().object_group(ObjRef::Solid(a)), Some(g1));
    let g3 = doc.group(&[ObjRef::Solid(a)]).expect("regroup");
    assert_eq!(doc.scene().object_group(ObjRef::Solid(a)), Some(g3));
    assert_eq!(doc.scene().object_group(ObjRef::Solid(b)), Some(g1));
    assert!(doc.group(&[]).is_err());
    assert_eq!(doc.ungroup(&[ObjRef::Solid(c)]).expect("none"), 0);
}

#[test]
fn entity_keys_and_outputs() {
    let mut doc = Document::new_empty();
    let a = doc.create_point_entity("logic_relay", DVec3::ZERO, &[("targetname".into(), "relay".into())]).expect("a");
    let b = doc.create_point_entity("logic_relay", DVec3::X * 32.0, &[]).expect("b");
    let n = check_undo_redo(&mut doc, |d| d.set_entity_key(&[a, b], "spawnflags", "1").expect("key"));
    assert_eq!(n, 2);
    assert_eq!(doc.set_entity_key(&[a, b], "spawnflags", "1").expect("same"), 0);
    check_undo_redo(&mut doc, |d| d.rename_entity_key(&[a, b], "spawnflags", "flags").expect("rename"));
    assert_eq!(doc.scene().entity(a).expect("a").entity.get("flags"), Some("1"));
    check_undo_redo(&mut doc, |d| d.remove_entity_key(&[a], "flags").expect("remove"));
    assert!(doc.scene().entity(a).expect("a").entity.get("flags").is_none());
    check_undo_redo(&mut doc, |d| d.set_entity_class(&[b], "logic_timer").expect("class"));
    assert_eq!(doc.set_entity_key(&[b], "classname", "logic_auto").expect("class via key"), 1);
    assert_eq!(doc.scene().entity(b).expect("b").entity.classname, "logic_auto");
    assert!(doc.set_entity_key(&[b], "id", "5").is_err());
    assert!(doc.set_entity_key(&[999], "x", "y").is_err());
    let conn = Connection::parse("OnTrigger", "relay,Trigger,,0,-1");
    check_undo_redo(&mut doc, |d| d.add_connection(&[b], &conn).expect("add"));
    let mut edited = conn.clone();
    edited.delay = 2.5;
    check_undo_redo(&mut doc, |d| d.set_connection(b, 0, edited.clone()).expect("edit"));
    assert_eq!(doc.scene().entity(b).expect("b").entity.connections[0].delay, 2.5);
    assert!(doc.set_connection(b, 5, edited.clone()).is_err());
    assert_eq!(doc.outputs_targeting("RELAY"), vec![(b, 0)]);
    let n = check_undo_redo(&mut doc, |d| d.rename_targetname("relay", "relay2", true).expect("rename"));
    assert_eq!(n, 1);
    assert_eq!(doc.scene().entity(b).expect("b").entity.connections[0].target, "relay2");
    assert_eq!(doc.entities_named("relay2"), vec![a]);
    let removed =
        check_undo_redo(&mut doc, |d| d.remove_connections_where(&[b], &|c| c.input == "Trigger").expect("rm"));
    assert_eq!(removed, 1);
    doc.set_connections(b, vec![conn.clone(), conn.clone()]).expect("set all");
    check_undo_redo(&mut doc, |d| d.remove_connection(b, 1).expect("rm one"));
    assert_eq!(doc.scene().entity(b).expect("b").entity.connections.len(), 1);
    assert!(doc.remove_connection(b, 9).is_err());
    assert!(doc.add_connection(&[b], &Connection::default()).is_err());
    let q = EntityQuery { classname: Some("logic_*".into()), ..EntityQuery::default() };
    assert_eq!(doc.find_entities(&q), vec![a, b]);
    let q = EntityQuery { targetname: Some("relay?".into()), ..EntityQuery::default() };
    assert_eq!(doc.find_entities(&q), vec![a]);
    let q = EntityQuery { key: Some("origin".into()), value: Some("32 *".into()), ..EntityQuery::default() };
    assert_eq!(doc.find_entities(&q), vec![b]);
}

#[test]
fn displacements() {
    let (mut doc, id) = doc_with_cube();
    let top = FaceRef::new(id, 0);
    let n = check_undo_redo(&mut doc, |d| d.create_displacements(&[top], 3).expect("create"));
    assert_eq!(n, 1);
    let mesh = doc.scene().solid(id).expect("s").disp_mesh(0).expect("mesh").clone();
    assert_eq!(mesh.row_len, 9);
    assert_eq!(doc.disp_faces(), vec![top]);
    assert!(doc.create_displacements(&[top], 5).is_err());
    let brush = PaintBrush {
        center: DVec3::new(32.0, 32.0, 64.0),
        radius: 24.0,
        amount: 8.0,
        mode: PaintMode::Raise,
        falloff: Falloff::Smooth,
        spatial: false,
    };
    let before = vmf(&doc);
    doc.begin("Sculpt");
    let mut modified = 0;
    for _ in 0..3 {
        modified += doc.disp_paint(&[], &brush).expect("paint");
    }
    doc.commit();
    assert!(modified > 0);
    assert_eq!(doc.history_len().0, 2);
    let raised = doc.scene().solid(id).expect("s").disp_mesh(0).expect("mesh").aabb.max.z;
    assert!(raised > 64.0 + 8.0, "{raised}");
    let hit = doc
        .pick_ray(
            &hp_geom::Ray::new(DVec3::new(32.0, 32.0, 500.0), DVec3::NEG_Z),
            &crate::PickOptions::default(),
            &no_bounds,
        )
        .expect("hit");
    assert_eq!(hit.face, Some(0));
    assert!(hit.point.z > 64.0 + 8.0);
    assert!(doc.undo());
    assert_eq!(vmf(&doc), before);
    assert!(doc.redo());
    let far = PaintBrush { center: DVec3::splat(5000.0), ..brush };
    assert_eq!(doc.disp_paint(&[top], &far).expect("far"), 0);
    assert!(doc.disp_paint(&[top], &PaintBrush { radius: -1.0, ..brush }).is_err());
    let n = check_undo_redo(&mut doc, |d| d.destroy_displacements(&[top]).expect("destroy"));
    assert_eq!(n, 1);
    assert!(doc.scene().solid(id).expect("s").disp_mesh(0).is_none());
    let mut doc = Document::new_empty();
    let wedge = PrimitiveParams { kind: Primitive::Wedge, ..PrimitiveParams::default() };
    let w = doc.create_primitive(&wedge, Owner::World).expect("wedge")[0];
    let tri = doc
        .scene()
        .solid(w)
        .expect("w")
        .geom
        .faces
        .iter()
        .find(|f| f.vertices.len() == 3)
        .map(|f| FaceRef::new(w, f.side_index))
        .expect("triangle");
    assert!(doc.create_displacements(&[tri], 2).is_err());
}

#[test]
fn edit_closures() {
    let (mut doc, id) = doc_with_cube();
    let e = doc.create_point_entity("info_target", DVec3::ZERO, &[]).expect("e");
    let changed = check_undo_redo(&mut doc, |d| {
        d.edit_entity(e, |x| {
            x.editor.comments = Some("hello".into());
            x.id = 999;
            x.props.push(("id".into(), "5".into()));
        })
        .expect("edit")
    });
    assert!(changed);
    let ent = &doc.scene().entity(e).expect("e").entity;
    assert_eq!(ent.id, e);
    assert!(ent.get("id").is_none());
    assert!(!doc.edit_entity(e, |_| {}).expect("noop"));
    assert!(doc.edit_entity(e, |x| x.classname.clear()).is_err());
    assert!(doc.edit_entity(4242, |_| {}).is_err());
    let changed = check_undo_redo(&mut doc, |d| d.edit_solid(id, |s| s.editor.color = [1, 2, 3]).expect("edit"));
    assert!(changed);
    let before = vmf(&doc);
    assert!(doc.edit_solid(id, |s| s.sides.truncate(2)).is_err());
    assert_eq!(vmf(&doc), before);
}

fn volume_of(doc: &Document, ids: &[crate::Id]) -> f64 {
    ids.iter().map(|id| doc.scene().solid(*id).expect("solid").geom.volume()).sum()
}

#[test]
fn intersect_keeps_the_shared_volume_in_the_first_id() {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::splat(32.0), 64.0), Owner::World).expect("b");
    let r = check_undo_redo(&mut doc, |d| d.intersect_solids(&[a, b]).expect("intersect"));
    assert_eq!(r, a);
    assert!(doc.scene().solid(b).is_none());
    assert!((volume_of(&doc, &[a]) - 32.0f64.powi(3)).abs() < 1e-6);
    let c = doc.create_solid(cube(DVec3::splat(500.0), 16.0), Owner::World).expect("c");
    assert!(doc.intersect_solids(&[a, c]).is_err());
    assert!(doc.intersect_solids(&[a]).is_err());
}

#[test]
fn union_joins_overlapping_boxes_and_keeps_l_shapes_disjoint() {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::new(32.0, 0.0, 0.0), 64.0), Owner::World).expect("b");
    let r = check_undo_redo(&mut doc, |d| d.union_solids(&[a, b]).expect("union"));
    assert_eq!(r, vec![a]);
    assert!(doc.scene().solid(b).is_none());
    assert!((volume_of(&doc, &[a]) - 96.0 * 64.0 * 64.0).abs() < 1e-6);

    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::new(32.0, 32.0, 0.0), 64.0), Owner::World).expect("b");
    let expected = 64.0f64.powi(3) * 2.0 - 32.0 * 32.0 * 64.0;
    let r = check_undo_redo(&mut doc, |d| d.union_solids(&[a, b]).expect("union"));
    assert!(r.len() >= 2);
    assert!((volume_of(&doc, &r) - expected).abs() < 1e-6);
    assert!(r.contains(&a) && r.contains(&b));
    assert!(doc.union_solids(&r).is_err());
}
