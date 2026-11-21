use glam::DVec3;

use super::{check_undo_redo, cube, vmf};
use crate::{Document, ObjRef, Owner, SelectOp, VisgroupState};

fn three() -> (Document, crate::Id, crate::Id, crate::Id) {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 16.0), Owner::World).expect("a");
    let b = doc.create_solid(cube(DVec3::new(32.0, 0.0, 0.0), 16.0), Owner::World).expect("b");
    let e = doc.create_brush_entity("func_wall", &[], vec![cube(DVec3::new(64.0, 0.0, 0.0), 16.0)]).expect("e");
    doc.clear_selection();
    doc.clear_history();
    (doc, a, b, e)
}

#[test]
fn hide_and_unhide() {
    let (mut doc, a, b, e) = three();
    let es = doc.scene().entity_solid_ids(e)[0];
    doc.select(&[ObjRef::Solid(a), ObjRef::Entity(e)], SelectOp::Replace);
    let n = check_undo_redo(&mut doc, |d| d.hide_selection());
    assert_eq!(n, 2);
    assert!(!doc.is_visible(ObjRef::Solid(a)));
    assert!(!doc.is_solid_visible(es), "brush entity solids follow their entity");
    assert!(doc.is_visible(ObjRef::Solid(b)));
    assert!(doc.selection().is_empty(), "hidden objects are deselected");
    assert!(vmf(&doc).contains("hidden"));
    let n = check_undo_redo(&mut doc, |d| d.unhide_all());
    assert_eq!(n, 2);
    assert!(doc.is_visible(ObjRef::Solid(a)) && doc.is_visible(ObjRef::Entity(e)));
    doc.select(&[ObjRef::Solid(b)], SelectOp::Replace);
    assert_eq!(doc.hide_unselected(), 2);
    assert!(doc.is_visible(ObjRef::Solid(b)) && !doc.is_visible(ObjRef::Solid(a)));
    doc.select_all();
    assert_eq!(doc.selected_objects(), vec![ObjRef::Solid(b)]);
    assert_eq!(doc.hide_selection(), 1);
    assert_eq!(doc.hide_selection(), 0);
}

#[test]
fn visgroups() {
    let (mut doc, a, b, e) = three();
    let v = check_undo_redo(&mut doc, |d| {
        d.create_visgroup("Walls", None, &[ObjRef::Solid(a), ObjRef::Entity(e)]).expect("create")
    });
    assert_eq!(doc.visgroup_members(v), vec![ObjRef::Solid(a), ObjRef::Entity(e)]);
    let child = doc.create_visgroup("Detail", Some(v), &[ObjRef::Solid(b)]).expect("child");
    assert_eq!(doc.visgroup_parent(child).expect("parent"), Some(v));
    assert!(doc.create_visgroup("Bad", Some(4242), &[]).is_err());
    check_undo_redo(&mut doc, |d| d.set_visgroup_visible(v, false).expect("hide"));
    assert!(!doc.is_visible(ObjRef::Solid(a)));
    assert!(!doc.is_visible(ObjRef::Solid(b)));
    assert!(!doc.is_visible(ObjRef::Entity(e)));
    assert_eq!(doc.visgroup_state(v), VisgroupState::Hidden);
    assert_eq!(doc.visgroup_state(child), VisgroupState::Hidden);
    doc.set_visgroup_visible(v, true).expect("show");
    doc.set_visgroup_visible(child, false).expect("hide child");
    assert_eq!(doc.visgroup_state(v), VisgroupState::Partial);
    assert!(doc.is_visible(ObjRef::Solid(a)) && !doc.is_visible(ObjRef::Solid(b)));
    assert!(doc.toggle_visgroup(child).expect("toggle"));
    assert!(doc.is_visible(ObjRef::Solid(b)));
    check_undo_redo(&mut doc, |d| d.add_to_visgroup(child, &[ObjRef::Solid(a)]).expect("add"));
    assert_eq!(doc.visgroup_member_counts().get(&child), Some(&2));
    check_undo_redo(&mut doc, |d| d.remove_from_visgroup(child, &[ObjRef::Solid(a)]).expect("remove"));
    check_undo_redo(&mut doc, |d| d.rename_visgroup(child, "Props").expect("rename"));
    assert_eq!(doc.visgroup(child).map(|g| g.name.as_str()), Some("Props"));
    doc.set_visgroup_color(child, [9, 9, 9]).expect("color");
    check_undo_redo(&mut doc, |d| d.move_visgroup(child, None).expect("move"));
    assert_eq!(doc.visgroup_parent(child).expect("parent"), None);
    assert!(doc.move_visgroup(v, Some(v)).is_err());
    doc.set_visgroup_visible(child, false).expect("hide");
    check_undo_redo(&mut doc, |d| d.delete_visgroup(child).expect("delete"));
    assert!(doc.visgroup(child).is_none());
    assert!(doc.is_visible(ObjRef::Solid(b)), "objects of a deleted visgroup are shown again");
    assert!(doc.delete_visgroup(child).is_err());
    doc.set_visgroup_visible(v, false).expect("hide");
    doc.show_all_visgroups();
    assert!(doc.is_visible(ObjRef::Solid(a)));
    assert!(doc.scene().hidden_visgroups().is_empty());
    check_undo_redo(&mut doc, |d| d.hide_all_visgroups());
    doc.hide_all_visgroups();
    assert!(!doc.is_visible(ObjRef::Solid(a)));
    assert_eq!(doc.visgroup_state(v), VisgroupState::Hidden);
    doc.show_all_visgroups();
    assert!(doc.is_visible(ObjRef::Solid(a)));
}

#[test]
fn visgroup_state_survives_save() {
    let (mut doc, a, _, _) = three();
    let v = doc.create_visgroup("Hidden stuff", None, &[ObjRef::Solid(a)]).expect("create");
    doc.set_visgroup_visible(v, false).expect("hide");
    let reloaded = Document::from_map(hp_vmf::parse(&vmf(&doc)).expect("parse"));
    assert!(!reloaded.is_visible(ObjRef::Solid(a)));
    assert_eq!(reloaded.visgroup_state(v), VisgroupState::Hidden);
}

#[test]
fn auto_visgroups() {
    let (mut doc, a, b, e) = three();
    let is_brush_entity = |s: &crate::Scene, o: ObjRef| matches!(o, ObjRef::Entity(id) if s.is_brush_entity(id));
    let n = check_undo_redo(&mut doc, |d| d.set_auto_visibility(&is_brush_entity, false));
    assert_eq!(n, 1);
    assert!(!doc.is_visible(ObjRef::Entity(e)));
    assert!(doc.is_visible(ObjRef::Solid(a)) && doc.is_visible(ObjRef::Solid(b)));
    assert_eq!(doc.set_auto_visibility(&is_brush_entity, false), 0);
    assert_eq!(doc.set_auto_visibility(&is_brush_entity, true), 1);
    assert!(doc.is_visible(ObjRef::Entity(e)));
}

#[test]
fn hidden_groups_hide_members() {
    let (mut doc, a, b, _) = three();
    let g = doc.group(&[ObjRef::Solid(a), ObjRef::Solid(b)]).expect("group");
    let v = doc.create_visgroup("G", None, &[]).expect("visgroup");
    let mut map = doc.to_map();
    if let Some(grp) = map.world.groups.iter_mut().find(|x| x.id == g) {
        grp.editor.visgroup_ids.push(v);
        grp.editor.visgroup_shown = false;
    }
    let doc = Document::from_map(map);
    assert!(!doc.is_visible(ObjRef::Solid(a)));
    assert!(!doc.is_visible(ObjRef::Solid(b)));
}

#[test]
fn removing_a_brush_entity_from_a_visgroup_also_frees_its_solids() {
    let (mut doc, a, _b, e) = three();
    let es = doc.scene().entity_solid_ids(e)[0];
    let v = doc.create_visgroup("Walls", None, &[ObjRef::Solid(a), ObjRef::Entity(e), ObjRef::Solid(es)]).expect("v");
    let w = doc.create_visgroup("Other", None, &[ObjRef::Solid(es)]).expect("w");
    assert_eq!(doc.visgroups_of(&[ObjRef::Entity(e)]), vec![v, w]);
    let before = doc.history_len().0;
    let n = check_undo_redo(&mut doc, |d| d.remove_from_visgroup(v, &[ObjRef::Entity(e)]).expect("remove"));
    assert_eq!(n, 1);
    assert_eq!(doc.history_len().0, before + 1);
    assert_eq!(doc.visgroup_members(v), vec![ObjRef::Solid(a)]);
    assert_eq!(doc.visgroups_of(&[ObjRef::Entity(e)]), vec![w]);
    let n = check_undo_redo(&mut doc, |d| {
        d.remove_from_all_visgroups(&[ObjRef::Entity(e), ObjRef::Solid(a)]).expect("all")
    });
    assert_eq!(n, 2);
    assert!(doc.visgroups_of(&[ObjRef::Entity(e), ObjRef::Solid(a)]).is_empty());
    assert!(doc.visgroup_members(w).is_empty());
    assert!(doc.remove_from_all_visgroups(&[ObjRef::Solid(a)]).is_err());
}

#[test]
fn point_entities_leave_visgroups() {
    let (mut doc, _a, _b, _e) = three();
    let p = doc.create_point_entity("light", DVec3::ZERO, &[]).expect("p");
    let v = doc.create_visgroup("Lights", None, &[ObjRef::Entity(p)]).expect("v");
    doc.remove_from_visgroup(v, &[ObjRef::Entity(p)]).expect("remove");
    assert!(doc.visgroup_members(v).is_empty());
}
