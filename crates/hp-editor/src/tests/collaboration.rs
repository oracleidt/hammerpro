use std::sync::Arc;

use super::*;

#[test]
fn remote_map_keeps_local_path_selection_and_unchanged_geometry() {
    let (mut doc, id) = doc_with_cube();
    doc.set_path(Some("local-only.vmf".into()));
    doc.select(&[ObjRef::Solid(id)], crate::SelectOp::Replace);
    doc.take_changes();
    let geometry = doc.scene().solids().get(&id).unwrap().geom.clone();
    let revision = doc.revision();
    let serial = doc.content_serial();
    let mut map = doc.to_map();
    map.entities.push(Entity {
        id: 20,
        classname: "light".into(),
        props: vec![("origin".into(), "10 20 30".into())],
        ..Default::default()
    });
    doc.apply_shared_map(map, 1_000_000, 2_000_000).unwrap();
    assert_eq!(doc.path().unwrap().to_str(), Some("local-only.vmf"));
    assert!(doc.is_selected(ObjRef::Solid(id)));
    assert!(Arc::ptr_eq(&geometry, &doc.scene().solids().get(&id).unwrap().geom));
    assert_ne!(revision, doc.revision());
    assert_ne!(serial, doc.content_serial());
    let changes = doc.take_changes();
    assert!(!changes.everything);
    assert!(changes.solids_changed.is_empty());
    assert!(changes.entities_changed.contains(&20));
    assert!((1_000_000..2_000_000).contains(&doc.peek_next_object_id()));
    assert!(!doc.can_undo());
}

#[test]
fn remote_map_does_not_interrupt_a_local_transaction() {
    let (mut doc, _) = doc_with_cube();
    let map = doc.to_map();
    let before = vmf(&doc);
    doc.begin("local drag");
    assert!(matches!(doc.apply_shared_map(map, 1_000_000, 2_000_000), Err(crate::EditError::TransactionOpen)));
    assert!(doc.in_transaction());
    assert_eq!(vmf(&doc), before);
    doc.cancel();
}

#[test]
fn foreign_identifiers_do_not_move_the_local_allocation_range() {
    let (mut doc, _) = doc_with_cube();
    let mut map = doc.to_map();
    map.entities.push(Entity { id: 8_000_010, classname: "light".into(), ..Default::default() });
    doc.apply_shared_map(map, 1_000_000, 2_000_000).unwrap();
    let id = doc.create_solid(cube(DVec3::splat(128.0), 64.0), Owner::World).unwrap();
    assert!((1_000_000..2_000_000).contains(&id));
    assert!(
        doc.scene().solids().get(&id).unwrap().solid.sides.iter().all(|side| (1_000_000..2_000_000).contains(&side.id))
    );
}
