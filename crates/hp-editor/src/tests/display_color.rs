use glam::DVec3;

use super::{check_undo_redo, cube, vmf};
use crate::display_color::{
    DisplayColor, DisplayColorState, PALETTE, UNDO_LABEL, chosen, custom_color, is_default_color, random_color,
};
use crate::{Document, ObjRef, Owner};

struct Fixture {
    doc: Document,
    a: u32,
    b: u32,
    light: u32,
    trigger: u32,
    trigger_solid: u32,
}

fn fixture() -> Fixture {
    let mut doc = Document::new_empty();
    let a = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).unwrap();
    let b = doc.create_solid(cube(DVec3::new(128.0, 0.0, 0.0), 64.0), Owner::World).unwrap();
    let light = doc.create_point_entity("light", DVec3::new(0.0, 0.0, 128.0), &[]).unwrap();
    let trigger = doc.create_brush_entity("trigger_once", &[], vec![cube(DVec3::new(0.0, 256.0, 0.0), 32.0)]).unwrap();
    let trigger_solid = doc.scene().entity_solid_ids(trigger)[0];
    doc.clear_history();
    doc.clear_selection();
    Fixture { doc, a, b, light, trigger, trigger_solid }
}

#[test]
fn default_colors_are_recognised() {
    assert!(is_default_color([0, 180, 255]));
    assert!(is_default_color([0, 100, 100]));
    assert!(is_default_color([220, 30, 220]));
    assert!(!is_default_color([0, 99, 200]));
    assert!(!is_default_color([1, 180, 255]));
    assert_eq!(custom_color([0, 180, 255]), [1, 180, 255]);
    assert_eq!(custom_color([220, 30, 220]), [221, 30, 220]);
    assert_eq!(custom_color([12, 34, 56]), [12, 34, 56]);
    assert!(PALETTE.iter().all(|c| chosen(*c) == Some(*c)), "every preset reads as a chosen color");
    for i in 0..64 {
        assert!(chosen(random_color(7, i)).is_some());
    }
    let first: std::collections::HashSet<[u8; 3]> = (0..8).map(|i| random_color(99, i)).collect();
    assert_eq!(first.len(), 8, "neighbours get different colors");
}

#[test]
fn set_is_one_undo_step_on_brushes_and_entities() {
    let mut f = fixture();
    let objs = [ObjRef::Solid(f.a), ObjRef::Entity(f.light), ObjRef::Entity(f.trigger)];
    let n = check_undo_redo(&mut f.doc, |d| d.set_display_color(&objs, DisplayColor::Set([40, 165, 225]), false));
    assert_eq!(n.unwrap(), 4, "the world brush, the light, the trigger and its brush");
    assert_eq!(f.doc.undo_label(), Some(UNDO_LABEL));
    assert_eq!(f.doc.history_len().0, 1);
    let scene = f.doc.scene();
    assert_eq!(scene.solid(f.a).unwrap().solid.editor.color, [40, 165, 225]);
    assert_eq!(scene.entity(f.light).unwrap().entity.editor.color, [40, 165, 225]);
    assert_eq!(scene.solid(f.trigger_solid).unwrap().solid.editor.color, [40, 165, 225]);
    assert!(is_default_color(scene.solid(f.b).unwrap().solid.editor.color), "untouched");
    assert_eq!(f.doc.display_color_state(&objs), DisplayColorState::Custom([40, 165, 225]));
    assert_eq!(f.doc.set_display_color(&objs, DisplayColor::Set([40, 165, 225]), false).unwrap(), 0);
    assert_eq!(f.doc.history_len().0, 1, "no step when nothing changes");
}

#[test]
fn live_drags_merge_into_one_step() {
    let mut f = fixture();
    let objs = [ObjRef::Solid(f.a)];
    for g in 0..5u8 {
        f.doc.set_display_color(&objs, DisplayColor::Set([200, g * 10, 30]), true).unwrap();
    }
    assert_eq!(f.doc.history_len().0, 1);
    assert!(f.doc.undo());
    assert!(is_default_color(f.doc.scene().solid(f.a).unwrap().solid.editor.color));
}

#[test]
fn states_default_custom_and_mixed() {
    let mut f = fixture();
    let a = [ObjRef::Solid(f.a)];
    let both = [ObjRef::Solid(f.a), ObjRef::Solid(f.b)];
    assert_eq!(f.doc.display_color_state(&[]), DisplayColorState::Empty);
    assert_eq!(f.doc.display_color_state(&both), DisplayColorState::Default);
    f.doc.set_display_color(&a, DisplayColor::Set([240, 140, 40]), false).unwrap();
    assert_eq!(f.doc.display_color_state(&a), DisplayColorState::Custom([240, 140, 40]));
    assert_eq!(f.doc.display_color_state(&both), DisplayColorState::Mixed);
    assert_eq!(f.doc.display_color_of(ObjRef::Solid(f.a)), Some(Some([240, 140, 40])));
    assert_eq!(f.doc.display_color_of(ObjRef::Solid(f.b)), Some(None));
    assert_eq!(f.doc.display_color_of(ObjRef::Solid(9999)), None);
    f.doc.set_display_color(&[ObjRef::Entity(f.trigger)], DisplayColor::Set([50, 180, 80]), false).unwrap();
    assert_eq!(f.doc.display_color_of(ObjRef::Solid(f.trigger_solid)), Some(Some([50, 180, 80])));
}

#[test]
fn random_then_reset() {
    let mut f = fixture();
    let objs = [ObjRef::Solid(f.a), ObjRef::Solid(f.b), ObjRef::Entity(f.light), ObjRef::Entity(f.trigger)];
    check_undo_redo(&mut f.doc, |d| d.set_display_color(&objs, DisplayColor::Random, false).unwrap());
    let scene = f.doc.scene();
    let ca = scene.solid(f.a).unwrap().solid.editor.color;
    let cb = scene.solid(f.b).unwrap().solid.editor.color;
    assert!(chosen(ca).is_some() && chosen(cb).is_some() && ca != cb);
    let te = scene.entity(f.trigger).unwrap().entity.editor.color;
    assert_eq!(scene.solid(f.trigger_solid).unwrap().solid.editor.color, te, "a brush entity is one object");
    assert_eq!(f.doc.display_color_state(&objs), DisplayColorState::Mixed);
    check_undo_redo(&mut f.doc, |d| d.set_display_color(&objs, DisplayColor::Reset, false).unwrap());
    assert_eq!(f.doc.display_color_state(&objs), DisplayColorState::Default);
    assert_eq!(f.doc.scene().entity(f.light).unwrap().entity.editor.color, [220, 30, 220]);
}

#[test]
fn groups_color_every_member_and_the_group() {
    let mut f = fixture();
    let members = [ObjRef::Solid(f.a), ObjRef::Entity(f.light)];
    let g = f.doc.group(&members).unwrap();
    f.doc.select_picked(ObjRef::Solid(f.a), crate::SelectMode::Groups, crate::SelectOp::Replace);
    let sel = f.doc.selected_objects();
    assert_eq!(sel.len(), 2, "picking one member selects the group");
    f.doc.set_display_color(&sel, DisplayColor::Set([170, 90, 220]), false).unwrap();
    let scene = f.doc.scene();
    assert_eq!(scene.group(g).unwrap().group.editor.color, [170, 90, 220]);
    assert_eq!(scene.entity(f.light).unwrap().entity.editor.color, [170, 90, 220]);
    f.doc.set_display_color(&[ObjRef::Solid(f.a)], DisplayColor::Set([1, 2, 3]), false).unwrap();
    assert_eq!(f.doc.scene().group(g).unwrap().group.editor.color, [170, 90, 220], "a partial pick keeps it");
    f.doc.set_display_color(&sel, DisplayColor::Reset, false).unwrap();
    assert!(is_default_color(f.doc.scene().group(g).unwrap().group.editor.color));
}

#[test]
fn colors_round_trip_through_the_vmf() {
    let mut f = fixture();
    f.doc.set_display_color(&[ObjRef::Solid(f.a)], DisplayColor::Set([75, 120, 235]), false).unwrap();
    f.doc.set_display_color(&[ObjRef::Entity(f.trigger)], DisplayColor::Set([235, 115, 150]), false).unwrap();
    let text = vmf(&f.doc);
    assert!(text.contains("\"color\" \"75 120 235\""), "{text}");
    let back = Document::from_map(hp_vmf::parse(&text).unwrap());
    assert_eq!(back.display_color_of(ObjRef::Solid(f.a)), Some(Some([75, 120, 235])));
    assert_eq!(back.display_color_of(ObjRef::Entity(f.trigger)), Some(Some([235, 115, 150])));
    assert_eq!(back.display_color_of(ObjRef::Solid(f.b)), Some(None));
    assert_eq!(vmf(&back), text);
}

#[test]
fn new_brushes_take_the_new_brush_color() {
    let mut doc = Document::new_empty();
    let plain = doc.create_solid(cube(DVec3::ZERO, 32.0), Owner::World).unwrap();
    assert!(is_default_color(doc.scene().solid(plain).unwrap().solid.editor.color));
    doc.options_mut().new_brush_color = Some([0, 200, 200]);
    let tinted = doc.create_solid(cube(DVec3::new(64.0, 0.0, 0.0), 32.0), Owner::World).unwrap();
    assert_eq!(doc.display_color_of(ObjRef::Solid(tinted)), Some(Some([1, 200, 200])));
}

#[test]
fn nothing_to_color_is_an_error() {
    let mut f = fixture();
    assert!(f.doc.set_display_color(&[], DisplayColor::Random, false).is_err());
    assert!(f.doc.set_display_color(&[ObjRef::Solid(4242)], DisplayColor::Reset, false).is_err());
    assert_eq!(f.doc.history_len().0, 0);
}
