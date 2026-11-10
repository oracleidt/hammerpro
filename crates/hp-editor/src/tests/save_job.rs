use std::time::Duration;

use glam::DVec3;

use super::cube;
use super::roundtrip::feature_map;
use crate::{Document, ObjRef, Owner, SelectOp};

#[test]
fn sliced_snapshot_equals_the_whole_map() {
    let mut doc = Document::from_map(feature_map());
    for i in 0..3000 {
        doc.create_solid(cube(DVec3::new(f64::from(i) * 64.0, 512.0, 0.0), 32.0), Owner::World).expect("create");
    }
    let mut job = doc.start_snapshot();
    let mut steps = 0;
    let map = loop {
        steps += 1;
        if let Some(map) = doc.step_snapshot(&mut job, Duration::ZERO) {
            break map;
        }
        assert!(steps < 100, "the snapshot ends");
    };
    assert!(steps > 1, "a zero budget slices the work");
    assert_eq!(map, doc.to_map());
}

#[test]
fn a_change_during_the_snapshot_starts_it_again() {
    let mut doc = Document::from_map(feature_map());
    for i in 0..3000 {
        doc.create_solid(cube(DVec3::new(f64::from(i) * 64.0, 512.0, 0.0), 32.0), Owner::World).expect("create");
    }
    let mut job = doc.start_snapshot();
    assert!(doc.step_snapshot(&mut job, Duration::ZERO).is_none());
    let added = doc.create_solid(cube(DVec3::new(0.0, -512.0, 0.0), 16.0), Owner::World).expect("create");
    let map = loop {
        if let Some(map) = doc.step_snapshot(&mut job, Duration::from_secs(60)) {
            break map;
        }
    };
    assert_eq!(job.restarts(), 1);
    assert!(map.world.solids.iter().any(|s| s.id == added), "the change is in the snapshot");
    assert_eq!(map, doc.to_map());
}

#[test]
fn background_save_marks_only_what_it_wrote() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-editor/scratch");
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let path = dir.join("background_save.vmf");
    let mut doc = Document::new_empty();
    doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("create");
    let version = doc.scene().version_info().map_version;
    let (map, ticket) = doc.begin_save(&path).expect("begin");
    assert_eq!(ticket.path(), path.as_path());
    assert_eq!(map.version_info.map_version, version + 1);
    doc.create_solid(cube(DVec3::splat(128.0), 64.0), Owner::World).expect("edit while writing");
    hp_vmf::save(&map, &path).expect("write");
    doc.finish_save(ticket, true);
    assert_eq!(doc.path(), Some(path.as_path()));
    assert!(doc.is_dirty(), "the edit made during the save is not on disk");
    assert!(doc.undo());
    assert!(!doc.is_dirty(), "back to what was written");
    let (_, ticket) = doc.begin_save(&path).expect("begin");
    doc.finish_save(ticket, false);
    assert!(doc.is_dirty(), "a failed save keeps the changes marked");
    let (_, ticket) = doc.begin_save(&path).expect("begin");
    doc.mark_dirty();
    doc.finish_save(ticket, true);
    assert!(doc.is_dirty(), "marked dirty after the snapshot");
    let (_, older) = doc.begin_save(&path).expect("begin");
    doc.create_solid(cube(DVec3::splat(512.0), 32.0), Owner::World).expect("edit");
    let (_, newer) = doc.begin_save(&path).expect("begin");
    doc.finish_save(newer, true);
    assert!(!doc.is_dirty());
    doc.finish_save(older, true);
    assert!(!doc.is_dirty(), "a save that finishes after a newer one changes nothing");
    let loaded = Document::open(&path).expect("open");
    assert_eq!(loaded.scene().solids().len(), 1, "the file holds what was written first");
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(path.with_extension("vmx"));
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-editor/scratch").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn a_save_as_during_a_background_save_keeps_the_new_file() {
    let dir = scratch("save_as_race");
    let (original, copy) = (dir.join("original.vmf"), dir.join("copy.vmf"));
    let mut doc = Document::new_empty();
    doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("create");
    doc.save(&original).expect("save");
    doc.create_solid(cube(DVec3::splat(128.0), 64.0), Owner::World).expect("edit");
    let (map, ticket) = doc.begin_save(&original).expect("begin");
    doc.save(&copy).expect("save as");
    assert_eq!(doc.path(), Some(copy.as_path()));
    assert!(!doc.is_dirty());
    hp_vmf::save(&map, &original).expect("write");
    doc.finish_save(ticket, true);
    assert_eq!(doc.path(), Some(copy.as_path()), "the Save As target stays the document file");
    assert!(!doc.is_dirty());
    let (_, ticket) = doc.begin_save(&original).expect("begin");
    doc.save(&copy).expect("save as");
    doc.finish_save(ticket, false);
    assert!(!doc.is_dirty(), "the older write failed but the newer save holds everything");
    assert_eq!(doc.path(), Some(copy.as_path()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_synchronous_save_after_a_background_save_keeps_the_dirty_flag_right() {
    let dir = scratch("sync_after_background");
    let path = dir.join("map.vmf");
    let mut doc = Document::new_empty();
    doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("create");
    doc.save(&path).expect("save");
    doc.create_solid(cube(DVec3::splat(128.0), 64.0), Owner::World).expect("first edit");
    let (map, ticket) = doc.begin_save(&path).expect("begin");
    doc.create_solid(cube(DVec3::splat(512.0), 32.0), Owner::World).expect("second edit");
    hp_vmf::save(&map, &path).expect("write");
    doc.save(&path).expect("synchronous save");
    assert!(!doc.is_dirty(), "the synchronous save wrote everything");
    doc.finish_save(ticket, true);
    assert!(!doc.is_dirty(), "the older background save changes nothing");
    assert_eq!(Document::open(&path).expect("open").scene().solids().len(), 3, "the file holds both edits");
    assert!(doc.undo());
    assert!(doc.is_dirty(), "undoing a saved edit makes the document differ from the file");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_failed_older_save_does_not_mark_a_newer_saved_state() {
    let dir = scratch("failed_older");
    let path = dir.join("map.vmf");
    let mut doc = Document::new_empty();
    doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("create");
    let (_, older) = doc.begin_save(&path).expect("begin");
    let (map, newer) = doc.begin_save(&path).expect("begin");
    doc.finish_save(older, false);
    hp_vmf::save(&map, &path).expect("write");
    doc.finish_save(newer, true);
    assert!(!doc.is_dirty(), "the newer save wrote everything");
    let (_, older) = doc.begin_save(&path).expect("begin");
    let (_, newer) = doc.begin_save(&path).expect("begin");
    doc.finish_save(older, false);
    doc.finish_save(newer, false);
    assert!(doc.is_dirty(), "both writes failed");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn selection_changes_do_not_restart_the_snapshot() {
    let mut doc = Document::new_empty();
    let mut ids = Vec::new();
    for i in 0..3000 {
        ids.push(
            doc.create_solid(cube(DVec3::new(f64::from(i) * 64.0, 0.0, 0.0), 32.0), Owner::World).expect("create"),
        );
    }
    doc.set_record_selection(true);
    let scene_revision = doc.scene_revision();
    let mut job = doc.start_snapshot();
    let mut steps = 0;
    let map = loop {
        if let Some(map) = doc.step_snapshot(&mut job, Duration::ZERO) {
            break map;
        }
        doc.select(&[ObjRef::Solid(ids[steps % 2])], SelectOp::Replace);
        doc.end_selection_burst();
        if steps == 2 {
            assert!(doc.undo(), "undo a selection step");
        }
        steps += 1;
        assert!(steps < 100, "the snapshot ends");
    };
    assert_eq!(job.restarts(), 0, "selection is not part of the saved map");
    assert_eq!(doc.scene_revision(), scene_revision);
    assert_eq!(map, doc.to_map());
}
