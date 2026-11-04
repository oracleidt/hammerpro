use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use glam::DVec3;
use hp_app::AppState;
use hp_app::runtime::BlenderTarget;
use hp_app::settings::BlenderFormat;
use hp_editor::{FaceRef, ObjRef, SelectOp};

use super::{SendOptions, find, pull, send};

const SCRIPT: &str = r##"import importlib.util
import os
import sys

import bpy
import mathutils

WORK = __WORK__
DELTA = __DELTA__

spec = importlib.util.spec_from_file_location("hammerpro_bridge", WORK + "/hammerpro_bridge.py")
mod = importlib.util.module_from_spec(spec)
sys.modules["hammerpro_bridge"] = mod
spec.loader.exec_module(mod)
mod.register()
link = mod.read_link()
mod.load_scene(link["file"], link["format"])
moved = 0
for obj in bpy.data.objects:
    if obj.type != "MESH" or not (obj.name.startswith("HPDISP_") or __ANY__):
        continue
    mesh = obj.data
    matrix = obj.matrix_world
    world = [matrix @ v.co for v in mesh.vertices]
    cx = sum(w.x for w in world) / len(world)
    cy = sum(w.y for w in world) / len(world)
    cz = sum(w.z for w in world) / len(world)
    best = min(range(len(world)), key=lambda i: (world[i].x - cx) ** 2 + (world[i].y - cy) ** 2 + (world[i].z - cz) ** 2)
    target = world[best] + mathutils.Vector((0.0, 0.0, DELTA))
    mesh.vertices[best].co = matrix.inverted() @ target
    mesh.update()
    moved += 1
print("E2E moved", moved, "objects", [o.name for o in bpy.data.objects])
bpy.ops.hammerpro.update()
print("E2E exported", link["file"])
mod.unregister()
"##;

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hp_blender_e2e_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    dir
}

pub fn blender_headless(work: &Path, delta: f64, any_object: bool) -> Result<String, String> {
    let exe = find::resolve("").ok_or("no blender")?;
    let script = SCRIPT
        .replace("__WORK__", &serde_json::to_string(&work.display().to_string()).unwrap_or_default())
        .replace("__DELTA__", &format!("{delta}"))
        .replace("__ANY__", if any_object { "True" } else { "False" });
    let path = work.join("e2e.py");
    std::fs::write(&path, script).map_err(|e| e.to_string())?;
    let mut child = std::process::Command::new(exe)
        .args(["-b", "--factory-startup", "--python-exit-code", "3", "--python"])
        .arg(&path)
        .env("HAMMERPRO_LINK", work.join("link.json"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > Duration::from_secs(120) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("blender timed out".to_owned());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if out.status.success() { Ok(text) } else { Err(text) }
}

fn displacement_state(work: &Path) -> (AppState, Vec<FaceRef>) {
    let mut state = hp_app::testing::test_state();
    state.runtime.blender.work_dir = Some(work.join("work"));
    let id =
        *state.doc.scene().solids().iter().find(|(_, n)| n.solid.sides.len() == 6).map(|(id, _)| id).expect("a box");
    let faces = [FaceRef::new(id, 0), FaceRef::new(id, 2)];
    state.doc.create_displacements(&faces, 2).expect("displacements");
    state.doc.select_faces(&faces, SelectOp::Replace);
    (state, faces.to_vec())
}

fn positions(state: &AppState, f: FaceRef) -> Vec<DVec3> {
    state.doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)).map(|m| m.positions.clone()).unwrap_or_default()
}

fn round_trip_displacements(format: BlenderFormat) {
    if find::resolve("").is_none() {
        return;
    }
    let work = scratch(format.id());
    let (mut state, faces) = displacement_state(&work);
    let before: Vec<Vec<DVec3>> = faces.iter().map(|f| positions(&state, *f)).collect();
    let alphas_before = state
        .doc
        .scene()
        .solid(faces[0].solid)
        .and_then(|n| n.solid.sides[faces[0].side].disp.as_ref().map(|d| d.alphas.clone()));
    let report =
        send(&mut state, &SendOptions { format: Some(format), launch: false, live: Some(true) }).expect("send");
    assert_eq!(report.kind, "displacements", "{format:?}");
    let BlenderTarget::Displacements { faces: sent } =
        state.runtime.blender.link.as_ref().expect("link").target.clone()
    else {
        panic!("displacements")
    };
    assert_eq!(sent.len(), 2);
    let log = blender_headless(&work.join("work"), 0.2, false).unwrap_or_else(|e| panic!("blender: {e}"));
    assert!(log.contains("E2E moved 2"), "{log}");
    let job = pull::job_of(&state).expect("job");
    let outcome = pull::compute(job).expect("compute");
    let steps = state.doc.history_len().0;
    let report = pull::apply(&mut state, outcome).expect("apply");
    assert_eq!(report.kind, "displacements", "{format:?}");
    assert_eq!(state.doc.history_len().0, steps + 1, "{format:?}: one undo step");
    let delta = 0.2 * hp_meshio::UNITS_PER_METER;
    for (k, f) in faces.iter().enumerate() {
        let after = positions(&state, *f);
        assert_eq!(after.len(), before[k].len());
        let moved: Vec<usize> = (0..after.len()).filter(|&i| (after[i] - before[k][i]).length() > 0.5).collect();
        assert!(!moved.is_empty(), "{format:?}: the sculpted vertex changed");
        let top = moved.iter().map(|&i| (after[i] - before[k][i]).z).fold(f64::MIN, f64::max);
        assert!((top - delta).abs() < 0.5, "{format:?}: raised by about {delta}, got {top}");
        assert_eq!(moved.len(), 1, "{format:?}: only the sculpted vertex moved");
    }
    let alphas_after = state
        .doc
        .scene()
        .solid(faces[0].solid)
        .and_then(|n| n.solid.sides[faces[0].side].disp.as_ref().map(|d| d.alphas.clone()));
    assert_eq!(alphas_before, alphas_after, "alpha is kept");
    assert!(state.doc.undo());
    for (k, f) in faces.iter().enumerate() {
        let restored = positions(&state, *f);
        assert!(
            restored.iter().zip(&before[k]).all(|(a, b)| (*a - *b).length() < 1e-9),
            "{format:?}: undo restores the shape"
        );
    }
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn displacements_round_trip_through_real_blender_as_glb() {
    round_trip_displacements(BlenderFormat::Gltf);
}

#[test]
fn displacements_round_trip_through_real_blender_as_obj() {
    round_trip_displacements(BlenderFormat::Obj);
}

#[test]
fn displacements_round_trip_through_real_blender_as_fbx() {
    round_trip_displacements(BlenderFormat::Fbx);
}

#[test]
fn brushes_round_trip_through_real_blender() {
    if find::resolve("").is_none() {
        return;
    }
    let work = scratch("brushes");
    let mut state = hp_app::testing::test_state();
    state.runtime.blender.work_dir = Some(work.join("work"));
    let id =
        *state.doc.scene().solids().iter().find(|(_, n)| n.solid.sides.len() == 6).map(|(id, _)| id).expect("a box");
    state.doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    let report = send(&mut state, &SendOptions { format: Some(BlenderFormat::Gltf), launch: false, live: Some(true) })
        .expect("send");
    assert_eq!(report.kind, "brushes");
    let log = blender_headless(&work.join("work"), 0.0, true).unwrap_or_else(|e| panic!("blender: {e}"));
    assert!(log.contains("E2E exported"), "{log}");
    let outcome = pull::compute(pull::job_of(&state).expect("job")).expect("compute");
    let report = pull::apply(&mut state, outcome).expect("apply");
    assert_eq!(report.kind, "brushes", "{}", report.text);
    assert!(state.doc.scene().solid(id).is_none(), "the sent brush was replaced");
    assert!(!report.solids.is_empty());
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn a_prop_exported_by_blender_compiles_into_a_model() {
    if find::resolve("").is_none() {
        return;
    }
    let work = scratch("prop");
    let mut state = hp_app::testing::test_state();
    let wd = work.join("work");
    state.runtime.blender.work_dir = Some(wd.clone());
    std::fs::create_dir_all(&wd).expect("work dir");
    let id =
        *state.doc.scene().solids().iter().find(|(_, n)| n.solid.sides.len() == 6).map(|(id, _)| id).expect("a box");
    state.doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    send(&mut state, &SendOptions { format: Some(BlenderFormat::Gltf), launch: false, live: Some(true) })
        .expect("send");
    let log = blender_headless(&wd, 0.0, true).unwrap_or_else(|e| panic!("blender: {e}"));
    assert!(log.contains("E2E exported"), "{log}");
    let link = state.runtime.blender.link.as_mut().expect("link");
    link.target =
        BlenderTarget::Prop { props: Vec::new(), model: "models/test/box.mdl".to_owned(), name: "e2e_box".to_owned() };
    let mut job = pull::job_of(&state).expect("job");
    let root = work.join("content");
    job.root = Some(root.clone());
    let outcome = pull::compute(job).expect("compute");
    let pull::Outcome::Prop { converted, name } = outcome else { panic!("a prop outcome") };
    assert_eq!(name, "e2e_box");
    assert!(converted.triangles > 0);
    assert!(root.join("models/hammerpro/e2e_box.mdl").is_file());
    let _ = std::fs::remove_dir_all(&work);
}

#[test]
fn the_detector_finds_the_installed_blender() {
    let Some(found) = find::detect() else { return };
    assert!(find::is_blender_exe(&found), "{}", found.display());
    assert_eq!(find::resolve(""), Some(found));
}

#[test]
#[ignore = "opens a Blender window for a few seconds"]
fn a_real_blender_window_opens_with_the_scene_and_is_closed_again() {
    let Some(exe) = find::resolve("") else { return };
    let work = scratch("launch");
    let mut state = hp_app::testing::test_state();
    state.runtime.blender.work_dir = Some(work.join("work"));
    let id =
        *state.doc.scene().solids().iter().find(|(_, n)| n.solid.sides.len() == 6).map(|(id, _)| id).expect("a box");
    state.doc.select(&[ObjRef::Solid(id)], SelectOp::Replace);
    send(&mut state, &SendOptions { format: Some(BlenderFormat::Gltf), launch: false, live: Some(true) })
        .expect("send");
    let wd = work.join("work");
    let mut child = super::launch(&exe, &wd.join("startup.py"), &wd.join("link.json")).expect("launch");
    std::thread::sleep(Duration::from_secs(10));
    assert!(child.try_wait().expect("status").is_none(), "Blender is still running");
    child.kill().expect("kill");
    let status = child.wait().expect("wait");
    assert!(!status.success() || status.code().is_some());
    let _ = std::fs::remove_dir_all(&work);
}
