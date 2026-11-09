#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod changes;
mod clipboard;
mod collaboration;
mod display_color;
mod fixes;
mod ops;
mod pick;
mod problems;
mod roundtrip;
mod selection;
mod selection_steps;
mod undo;
mod visibility;

use glam::DVec3;
use hp_geom::Aabb;
use hp_vmf::{Entity, Side, TexAxis};

use crate::{Document, Id, ObjRef, Owner};

pub(crate) fn box_sides(min: DVec3, max: DVec3, material: &str) -> Vec<Side> {
    let (x0, y0, z0) = (min.x, min.y, min.z);
    let (x1, y1, z1) = (max.x, max.y, max.z);
    let v = DVec3::new;
    let floor = (DVec3::X, DVec3::NEG_Y);
    let wall_y = (DVec3::X, DVec3::NEG_Z);
    let wall_x = (DVec3::Y, DVec3::NEG_Z);
    let faces = [
        ([v(x0, y1, z1), v(x1, y1, z1), v(x1, y0, z1)], floor),
        ([v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0)], floor),
        ([v(x1, y1, z1), v(x0, y1, z1), v(x0, y1, z0)], wall_y),
        ([v(x1, y0, z0), v(x0, y0, z0), v(x0, y0, z1)], wall_y),
        ([v(x1, y1, z0), v(x1, y0, z0), v(x1, y0, z1)], wall_x),
        ([v(x0, y1, z1), v(x0, y0, z1), v(x0, y0, z0)], wall_x),
    ];
    faces
        .iter()
        .map(|(plane, (u, w))| Side {
            id: 0,
            plane: *plane,
            material: material.to_owned(),
            uaxis: TexAxis { axis: *u, shift: 0.0, scale: 0.25 },
            vaxis: TexAxis { axis: *w, shift: 0.0, scale: 0.25 },
            ..Default::default()
        })
        .collect()
}

pub(crate) fn cube(min: DVec3, size: f64) -> Vec<Side> {
    box_sides(min, min + DVec3::splat(size), "DEV/DEV_MEASUREGENERIC01")
}

pub(crate) fn vmf(doc: &Document) -> String {
    hp_vmf::write(&doc.to_map())
}

pub(crate) fn no_bounds(_: &Entity) -> Option<Aabb> {
    None
}

pub(crate) fn check_undo_redo<R>(doc: &mut Document, op: impl FnOnce(&mut Document) -> R) -> R {
    let before = vmf(doc);
    let r = op(doc);
    let after = vmf(doc);
    assert_ne!(before, after, "operation did not change the document");
    assert!(doc.undo(), "undo failed");
    assert_eq!(vmf(doc), before, "undo did not restore the previous state");
    assert!(doc.redo(), "redo failed");
    assert_eq!(vmf(doc), after, "redo did not restore the result");
    r
}

pub(crate) fn doc_with_cube() -> (Document, Id) {
    let mut doc = Document::new_empty();
    let id = doc.create_solid(cube(DVec3::ZERO, 64.0), Owner::World).expect("create");
    doc.clear_history();
    doc.clear_selection();
    (doc, id)
}

pub(crate) fn bounds(doc: &Document, obj: ObjRef) -> Aabb {
    doc.object_bounds(obj, &no_bounds).expect("bounds")
}

pub(crate) fn sample_vmfs() -> Vec<std::path::PathBuf> {
    let dir =
        std::path::Path::new("D:/SteamLibrary/steamapps/common/GarrysMod/garrysmod/gamemodes/terrortown/mapexamples");
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("vmf")) {
                out.push(p);
            }
        }
    }
    let extra = std::path::Path::new("C:/Users/oracle/Desktop/hammermcptest/test.vmf");
    if extra.is_file() {
        out.push(extra.to_path_buf());
    }
    out.sort();
    out
}
