use std::sync::Arc;

use super::*;
use crate::SharedUpdate;

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

fn shared_fixture() -> (Document, Id) {
    let (doc, solid) = doc_with_cube();
    let mut map = doc.to_map();
    for id in [20, 21] {
        map.entities.push(Entity { id, classname: "prop_dynamic".into(), ..Default::default() });
    }
    (Document::from_map(map), solid)
}

fn sorted(mut ids: Vec<Id>) -> Vec<Id> {
    ids.sort_unstable();
    ids
}

#[test]
fn shared_journal_reports_every_touched_object_kind_and_skips_remote_updates() {
    let (mut doc, solid) = shared_fixture();
    let initial = doc.content_serial();
    doc.begin_merge("Move prop", "prop-nudge");
    doc.set_entity_key(&[20], "origin", "8 0 0").unwrap();
    doc.commit();
    let touched = doc.shared_touched_since(initial).unwrap();
    assert_eq!(touched.entities, [20]);
    assert!(touched.solids.is_empty() && !touched.world && !touched.metadata);
    let previous = doc.content_serial();
    doc.begin_merge("Move prop", "prop-nudge");
    doc.set_entity_key(&[20], "origin", "16 0 0").unwrap();
    doc.commit();
    assert_eq!(doc.shared_touched_since(previous).unwrap().entities, [20]);
    doc.translate(&[ObjRef::Solid(solid)], DVec3::X).unwrap();
    doc.set_world_key("skyname", "sky_day02_01").unwrap();
    doc.create_visgroup("Team", None, &[ObjRef::Solid(solid)]).unwrap();
    let touched = doc.shared_touched_since(previous).unwrap();
    assert_eq!(touched.solids, [solid]);
    assert!(touched.world && touched.metadata);
    let serial = doc.content_serial();
    let mut remote = doc.scene().entity(21).unwrap().entity.clone();
    remote.set("origin", "32 0 0");
    doc.apply_shared_update(SharedUpdate { entities: vec![(21, Some(remote))], ..Default::default() }).unwrap();
    assert_ne!(doc.content_serial(), serial);
    assert!(doc.shared_touched_since(serial).unwrap().is_empty());
    assert_eq!(doc.scene().entity(21).unwrap().entity.get("origin"), Some("32 0 0"));
    doc.begin("Drag");
    doc.set_entity_key(&[20], "origin", "64 0 0").unwrap();
    assert!(doc.shared_touched_since(serial).is_none());
    assert_eq!(doc.shared_touched_live(serial).unwrap().entities, [20]);
    doc.commit();
    assert_eq!(doc.shared_touched_since(serial).unwrap().entities, [20]);
    assert!(doc.shared_touched_since(doc.content_serial() + 1).is_none());
}

#[test]
fn shared_updates_match_a_full_reinstall_and_keep_untouched_geometry() {
    let (mut local, solid) = shared_fixture();
    let (mut remote, _) = shared_fixture();
    local.take_changes();
    let untouched = local.scene().solid(solid).unwrap().geom.clone();
    let start = remote.content_serial();
    let created = remote.create_solid(cube(DVec3::splat(256.0), 32.0), Owner::World).unwrap();
    remote.set_entity_key(&[20], "targetname", "remote_prop").unwrap();
    remote.delete(&[ObjRef::Entity(21)]).unwrap();
    let detail = remote.create_solid(cube(DVec3::splat(-256.0), 16.0), Owner::World).unwrap();
    let entity = remote.tie_to_entity(&[ObjRef::Solid(detail)], "func_detail", &[]).unwrap();
    remote.group(&[ObjRef::Solid(created), ObjRef::Entity(entity)]).unwrap();
    remote.create_visgroup("Remote", None, &[ObjRef::Solid(created)]).unwrap();
    remote.set_world_key("skyname", "sky_day03_01").unwrap();
    let touched = remote.shared_touched_since(start).unwrap();
    let export = remote.shared_export(&touched);
    let world_id = remote.scene().world().id;
    let header = export.header.clone().unwrap();
    let update = SharedUpdate {
        world: Some(header.world.clone()),
        metadata: Some((header.visgroups.clone(), header.extra_blocks.clone())),
        entities: export
            .entities
            .iter()
            .map(|(id, row)| (*id, row.as_ref().map(|(entity, _)| entity.clone())))
            .collect(),
        solids: export
            .solids
            .iter()
            .map(|(id, row)| {
                let owner = row.as_ref().map(|(solid, owner)| {
                    (solid.clone(), if *owner == world_id { Owner::World } else { Owner::Entity(*owner) })
                });
                (*id, owner)
            })
            .collect(),
    };
    local.apply_shared_update(update).unwrap();
    let changes = local.take_changes();
    assert!(!changes.everything);
    assert!(changes.solids_changed.contains(&created) && changes.solids_changed.contains(&detail));
    assert!(!changes.solids_changed.contains(&solid));
    assert!(changes.entities_removed.contains(&21));
    assert!(std::sync::Arc::ptr_eq(&untouched, &local.scene().solid(solid).unwrap().geom));
    let mut reinstalled = Document::from_map(local.to_map());
    reinstalled.apply_shared_map(remote.to_map(), 1_000_000, 2_000_000).unwrap();
    let normalize = |doc: &Document| {
        let mut map = doc.to_map();
        map.world.solids.sort_by_key(|solid| solid.id);
        map.entities.sort_by_key(|entity| entity.id);
        map.world.groups.sort_by_key(|group| group.id);
        hp_vmf::write(&map)
    };
    assert_eq!(normalize(&local), normalize(&reinstalled));
    assert_eq!(local.scene().hidden_visgroups(), reinstalled.scene().hidden_visgroups());
    assert!(local.shared_touched_since(local.content_serial()).unwrap().is_empty());
}

#[test]
fn shared_updates_during_a_local_transaction_skip_its_objects() {
    let (mut doc, solid) = shared_fixture();
    doc.begin("Sculpt");
    doc.translate(&[ObjRef::Solid(solid)], DVec3::Z).unwrap();
    let mut prop = doc.scene().entity(21).unwrap().entity.clone();
    prop.set("origin", "0 0 99");
    doc.apply_shared_update(SharedUpdate { entities: vec![(21, Some(prop))], ..Default::default() }).unwrap();
    assert!(doc.in_transaction());
    let moved = doc.scene().solid(solid).unwrap().solid.clone();
    let conflicting = SharedUpdate { solids: vec![(solid, Some((moved, Owner::World)))], ..Default::default() };
    assert!(matches!(doc.apply_shared_update(conflicting), Err(crate::EditError::TransactionOpen)));
    doc.cancel();
    assert_eq!(doc.scene().entity(21).unwrap().entity.get("origin"), Some("0 0 99"));
}

#[test]
fn shared_point_batch_keeps_untouched_brushes_and_unsubmitted_entity_edits() {
    let (mut doc, solid) = shared_fixture();
    let initial = doc.content_serial();
    doc.set_entity_key(&[20], "origin", "8 0 0").unwrap();
    let pointer = doc.scene().solid(solid).unwrap() as *const _;
    doc.take_changes();
    let mut remote = doc.scene().entity(21).unwrap().entity.clone();
    remote.set("origin", "16 0 0");
    doc.apply_shared_update(SharedUpdate { entities: vec![(21, Some(remote))], ..Default::default() }).unwrap();
    assert_eq!(doc.scene().solid(solid).unwrap() as *const _, pointer);
    assert_eq!(doc.scene().entity(20).unwrap().entity.get("origin"), Some("8 0 0"));
    assert_eq!(sorted(doc.shared_touched_since(initial).unwrap().entities), [20]);
    let changes = doc.take_changes();
    assert!(!changes.everything && !changes.solids_dirty());
    assert_eq!(changes.entities_changed.into_iter().collect::<Vec<_>>(), [21]);
}

fn two_props_and_a_cube() -> (Document, u32) {
    let (doc, solid) = doc_with_cube();
    let mut map = doc.to_map();
    for id in [20, 21] {
        let props = [("origin", "0 0 0"), ("angles", "0 0 0"), ("targetname", "prop"), ("model", "models/a.mdl")];
        let props = props.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect();
        map.entities.push(Entity { id, classname: "prop_dynamic".into(), props, ..Default::default() });
    }
    let mut doc = Document::from_map(map);
    doc.set_undo_limits(crate::UndoLimits { merge_window: Some(std::time::Duration::ZERO), ..doc.undo_limits() });
    (doc, solid)
}

#[test]
fn shared_journal_reads_older_steps_that_keep_only_changed_keys() {
    let (mut doc, _) = two_props_and_a_cube();
    let initial = doc.content_serial();
    doc.set_entity_key(&[20], "origin", "8 0 0").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    doc.set_entity_key(&[21], "origin", "16 0 0").unwrap();
    assert_eq!(doc.history_len().0, 2, "two separate steps");
    let touched = doc.shared_touched_since(initial).expect("the journal covers both steps");
    let mut ids = touched.entities.clone();
    ids.sort_unstable();
    assert_eq!(ids, [20, 21], "the older step is shared too");
    assert!(touched.solids.is_empty() && !touched.world && !touched.metadata);
}

#[test]
fn shared_journal_reports_an_older_brush_step() {
    let (mut doc, solid) = two_props_and_a_cube();
    let initial = doc.content_serial();
    doc.translate(&[ObjRef::Solid(solid)], DVec3::new(16.0, 0.0, 0.0)).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(2));
    doc.set_entity_key(&[21], "origin", "16 0 0").unwrap();
    assert_eq!(doc.history_len().0, 2, "two separate steps");
    let touched = doc.shared_touched_since(initial).expect("the journal covers both steps");
    assert_eq!(touched.solids, [solid], "the moved brush of the older step is shared");
    assert_eq!(touched.entities, [21]);
}
