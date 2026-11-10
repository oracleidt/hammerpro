#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stderr)]

use std::path::PathBuf;
use std::time::Instant;

use glam::{DQuat, DVec3};
use hp_editor::Document;

fn timed<R>(label: &str, f: impl FnOnce() -> R) -> R {
    let t = Instant::now();
    let r = f();
    eprintln!("bench {label}: {:.1} ms", t.elapsed().as_secs_f64() * 1000.0);
    r
}

#[test]
#[ignore = "needs HP_BENCH_VMF pointing to a big map"]
fn editing_at_scale() {
    let Some(path) = std::env::var_os("HP_BENCH_VMF").map(PathBuf::from) else { return };
    let mut doc = timed("open", || Document::open(&path).unwrap());
    eprintln!("bench map: {} solids, {} entities", doc.scene().solids().len(), doc.scene().entities().len());
    timed("select_all", || doc.select_all());
    let objs = doc.selected_objects();
    eprintln!("bench selected: {}", objs.len());
    timed("translate", || doc.translate(&objs, DVec3::new(64.0, 0.0, 0.0)).unwrap());
    eprintln!("bench history after move: {:.1} MB", doc.history_bytes() as f64 / 1e6);
    let center = doc.selection_center(&|_| None).unwrap_or(DVec3::ZERO);
    timed("rotate", || doc.rotate(&objs, center, DQuat::from_rotation_z(15f64.to_radians())).unwrap());
    let from = doc.selection_bounds(&|_| None).unwrap();
    let mut to = from;
    to.max += (from.max - from.min) * 0.1;
    timed("scale", || doc.scale_bounds(&objs, &from, &to).unwrap());
    eprintln!("bench history after scale: {:.1} MB", doc.history_bytes() as f64 / 1e6);
    timed("undo", || assert!(doc.undo()));
    timed("redo", || assert!(doc.redo()));
    timed("undo3", || {
        doc.undo();
        doc.undo();
        doc.undo();
    });
    timed("redo3", || {
        doc.redo();
        doc.redo();
        doc.redo();
    });
    let _ = doc.take_changes();
    timed("clone", || {
        doc.duplicate_transformed(&objs, glam::DMat4::from_translation(DVec3::new(0.0, 128.0, 0.0)), center, true)
            .unwrap()
    });
    eprintln!("bench after clone: {} solids", doc.scene().solids().len());
    doc.select_all();
    let all = doc.selected_objects();
    timed("delete", || doc.delete_selection().unwrap());
    timed("undo_delete", || assert!(doc.undo()));
    eprintln!("bench history: {:.1} MB", doc.history_bytes() as f64 / 1e6);
    timed("clear_selection", || doc.clear_selection());
    let map = timed("to_map", || doc.to_map());
    let bytes = timed("write_bytes", || hp_vmf::write_bytes(&map).0.len());
    eprintln!("bench vmf bytes: {:.1} MB", bytes as f64 / 1e6);
    let plain = std::env::var_os("HP_BENCH_OUT")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .with_extension("plain.vmf");
    timed("save_no_sync_no_backup", || {
        hp_vmf::save_with(&map, &plain, &hp_vmf::SaveOptions { backup: false, sync: false }).unwrap()
    });
    let _ = std::fs::remove_file(&plain);
    let out = std::env::temp_dir().join("hp_scale_bench_out.vmf");
    let out = std::env::var_os("HP_BENCH_OUT").map(PathBuf::from).unwrap_or(out);
    timed("save", || doc.save(&out).unwrap());
    let _ = std::fs::remove_file(&out);
    let _ = all;
}
