use glam::{DQuat, DVec3};

use super::{cube, vmf};
use crate::{Document, FaceEdit, FaceRef, ObjRef, Owner, SELECTION_STEP, SelectOp};

fn busy_doc() -> Document {
    let mut doc = Document::new_empty();
    let mut ids = Vec::new();
    for i in 0..400 {
        let at = DVec3::new(f64::from(i % 20) * 64.0, f64::from(i / 20) * 64.0, 0.0);
        ids.push(doc.create_solid(cube(at, 48.0), Owner::World).expect("solid"));
    }
    let faces: Vec<FaceRef> = ids.iter().take(30).map(|id| FaceRef::new(*id, 0)).collect();
    doc.create_displacements(&faces, 3).expect("displacements");
    for i in 0..60 {
        let keys = [
            ("angles".to_owned(), "0 45 0".to_owned()),
            ("model".to_owned(), "models/props_c17/oildrum001.mdl".to_owned()),
            ("skin".to_owned(), "0".to_owned()),
        ];
        doc.create_point_entity("prop_static", DVec3::new(f64::from(i) * 24.0, 32.0, 8.0), &keys).expect("entity");
    }
    doc.clear_history();
    let _ = doc.take_changes();
    doc
}

fn everything(doc: &Document) -> Vec<ObjRef> {
    doc.scene()
        .solids()
        .keys()
        .map(|id| ObjRef::Solid(*id))
        .chain(doc.scene().entities().keys().map(|id| ObjRef::Entity(*id)))
        .collect()
}

#[test]
fn older_steps_keep_only_what_changed_and_undo_exactly() {
    let mut doc = busy_doc();
    let all = everything(&doc);
    let mut states = vec![vmf(&doc)];
    doc.translate(&all, DVec3::new(16.0, 8.0, 0.0)).expect("move");
    states.push(vmf(&doc));
    let one_move = doc.history_bytes();
    doc.rotate(&all, DVec3::new(100.0, 50.0, 0.0), DQuat::from_rotation_z(0.3)).expect("rotate");
    states.push(vmf(&doc));
    let faces: Vec<FaceRef> = doc.scene().solids().keys().map(|id| FaceRef::new(*id, 1)).collect();
    let edit = FaceEdit { material: Some("BRICK/BRICKWALL001A".into()), ..FaceEdit::default() };
    doc.edit_faces(&faces, &edit).expect("texture");
    states.push(vmf(&doc));
    let ents: Vec<u32> = doc.scene().entities().keys().copied().collect();
    doc.set_entity_key(&ents, "skin", "1").expect("keys");
    states.push(vmf(&doc));
    doc.translate(&all, DVec3::new(0.0, 0.0, 32.0)).expect("move up");
    states.push(vmf(&doc));
    assert!(
        doc.history_bytes() < one_move * 3,
        "five steps of the whole map take {} bytes, one move {one_move}",
        doc.history_bytes()
    );
    for expected in states.iter().rev().skip(1) {
        assert!(doc.undo());
        assert_eq!(&vmf(&doc), expected);
    }
    assert!(!doc.undo());
    for expected in states.iter().skip(1) {
        assert!(doc.redo());
        assert_eq!(&vmf(&doc), expected);
    }
    for expected in states.iter().rev().skip(1).take(3) {
        assert!(doc.undo());
        assert_eq!(&vmf(&doc), expected);
    }
    doc.translate(&all, DVec3::new(-8.0, 0.0, 0.0)).expect("branch");
    let branch = vmf(&doc);
    assert!(doc.undo());
    assert_eq!(&vmf(&doc), &states[2]);
    assert!(doc.undo());
    assert_eq!(&vmf(&doc), &states[1]);
    assert!(doc.redo());
    assert!(doc.redo());
    assert_eq!(vmf(&doc), branch);
}

#[test]
fn geometry_comes_back_with_the_undone_step() {
    let mut doc = busy_doc();
    let all = everything(&doc);
    let before: Vec<_> = doc.scene().solids().values().map(|n| (n.bounds, n.disp.len())).collect();
    doc.translate(&all, DVec3::new(64.0, 0.0, 0.0)).expect("move");
    doc.translate(&all, DVec3::new(0.0, 64.0, 0.0)).expect("move");
    assert!(doc.undo());
    assert!(doc.undo());
    let after: Vec<_> = doc.scene().solids().values().map(|n| (n.bounds, n.disp.len())).collect();
    assert_eq!(before, after);
    assert!(doc.scene().solids().values().all(|n| n.geom.valid));
}

#[test]
fn selection_steps_between_edits_do_not_keep_the_older_edit_whole() {
    let mut plain = busy_doc();
    let all = everything(&plain);
    plain.translate(&all, DVec3::new(16.0, 0.0, 0.0)).expect("move");
    let one_move = plain.history_bytes();
    plain.translate(&all, DVec3::new(0.0, 16.0, 0.0)).expect("move");
    let direct = plain.history_bytes();
    assert!(direct < one_move * 3 / 2, "two direct moves take {direct} bytes, one move {one_move}");

    let mut doc = busy_doc();
    doc.set_record_selection(true);
    let mut states = vec![vmf(&doc)];
    doc.select(&all, SelectOp::Replace);
    doc.end_selection_burst();
    doc.translate(&all, DVec3::new(16.0, 0.0, 0.0)).expect("move");
    states.push(vmf(&doc));
    for _ in 0..2 {
        doc.clear_selection();
        doc.end_selection_burst();
        doc.select(&all, SelectOp::Replace);
        doc.end_selection_burst();
    }
    let labels = doc.undo_labels();
    assert_eq!(labels.iter().rev().take(2).copied().collect::<Vec<_>>(), [SELECTION_STEP; 2], "{labels:?}");
    doc.translate(&all, DVec3::new(0.0, 16.0, 0.0)).expect("move");
    states.push(vmf(&doc));
    doc.clear_selection();
    doc.end_selection_burst();
    doc.translate(&all[..1], DVec3::new(0.0, 0.0, 8.0)).expect("small move");
    states.push(vmf(&doc));
    assert!(
        doc.history_bytes() < one_move,
        "three edits split by selection steps take {} bytes, one whole move {one_move}, two direct moves {direct}",
        doc.history_bytes()
    );
    let mut seen = vec![vmf(&doc)];
    while doc.undo() {
        let now = vmf(&doc);
        if seen.last() != Some(&now) {
            seen.push(now);
        }
    }
    seen.reverse();
    assert_eq!(seen, states, "undo walks back through every edit exactly");
    while doc.redo() {}
    assert_eq!(vmf(&doc), states[3]);
}
