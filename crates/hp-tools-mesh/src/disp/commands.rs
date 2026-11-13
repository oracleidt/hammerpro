use hp_app::state::AppState;
use hp_app::{Severity, t, tf};
use hp_editor::{Document, EditError, FaceRef};
use hp_geom::{DISP_FLAG_NO_HULL, DISP_FLAG_NO_PHYSICS, DISP_FLAG_NO_RAY};

use super::paint::destroy_targets;
use super::seams::{UNDO_SEW, plan_sew};
use super::smooth::{SmoothPart, subdivide_parts};
use super::subdivision::{SubdivisionShape, connected_parts, sphere_parts};
use crate::common::edit_error_text;

pub(crate) const UNDO_SUBDIVIDE: &str = "Subdivide displacements";

pub(crate) const COLLISION_FLAGS: [(i32, &str, &str); 3] = [
    (DISP_FLAG_NO_PHYSICS, "disp.flag.no_physics", "disp.flag.no_physics_tip"),
    (DISP_FLAG_NO_HULL, "disp.flag.no_hull", "disp.flag.no_hull_tip"),
    (DISP_FLAG_NO_RAY, "disp.flag.no_ray", "disp.flag.no_ray_tip"),
];

pub fn targets(doc: &Document) -> Vec<FaceRef> {
    destroy_targets(doc)
}

fn require(state: &mut AppState, faces: &[FaceRef]) -> bool {
    if faces.is_empty() {
        state.notify(Severity::Info, t!("disp.nothing_selected"));
        return false;
    }
    true
}

fn failed(state: &mut AppState, e: &EditError) {
    state.notify(Severity::Warning, tf!("disp.failed", edit_error_text(e)));
}

pub fn invert_alpha(state: &mut AppState) -> usize {
    let faces = targets(&state.doc);
    if !require(state, &faces) {
        return 0;
    }
    match state.doc.invert_disp_alpha(&faces) {
        Ok(n) => {
            state.notify(Severity::Success, tf!("disp.inverted", faces.len()));
            n
        }
        Err(e) => {
            failed(state, &e);
            0
        }
    }
}

pub fn add_noise(state: &mut AppState) -> usize {
    let faces = targets(&state.doc);
    if !require(state, &faces) {
        return 0;
    }
    let (min, max) = state.settings.mesh.noise_range();
    let seed = state.doc.revision() ^ 0x5eed_0fd1;
    match state.doc.disp_noise(&faces, min, max, seed) {
        Ok(n) => {
            state.notify(Severity::Success, tf!("disp.noised", faces.len()));
            n
        }
        Err(e) => {
            failed(state, &e);
            0
        }
    }
}

pub(crate) fn reshape_doc(
    doc: &mut Document,
    faces: &[FaceRef],
    label: &'static str,
    run: impl FnOnce(&mut [SmoothPart<'_>]) -> Vec<usize>,
) -> Result<usize, EditError> {
    reshape_doc_components(doc, faces, label, false, run)
}

fn reshape_doc_components(
    doc: &mut Document,
    faces: &[FaceRef],
    label: &'static str,
    separate: bool,
    run: impl FnOnce(&mut [SmoothPart<'_>]) -> Vec<usize>,
) -> Result<usize, EditError> {
    let mut work: Vec<(FaceRef, hp_geom::DispMesh, hp_vmf::DispInfo)> = Vec::new();
    for f in faces {
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let (Some(mesh), Some(disp)) =
            (node.disp_mesh(f.side), node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()))
        else {
            continue;
        };
        work.push((*f, (**mesh).clone(), disp.clone()));
    }
    let (counts, groups) = {
        let mut parts: Vec<SmoothPart<'_>> =
            work.iter_mut().map(|(_, mesh, disp)| SmoothPart { mesh, disp, pinned: false }).collect();
        let groups = separate.then(|| connected_parts(&parts));
        (run(&mut parts), groups)
    };
    let groups: Vec<Vec<FaceRef>> = groups.map_or_else(
        || vec![faces.to_vec()],
        |groups| groups.into_iter().map(|group| group.into_iter().map(|i| work[i].0).collect()).collect(),
    );
    let mut updates = Vec::new();
    let mut moved = 0;
    for ((f, _, mut d), n) in work.into_iter().zip(counts) {
        if n == 0 {
            continue;
        }
        if let Some(poly) = doc.scene().solid(f.solid).and_then(|node| node.geom.face_for_side(f.side)) {
            hp_geom::update_disp_tags(&mut d, &poly.vertices);
        }
        moved += n;
        updates.push((f, d));
    }
    if updates.is_empty() {
        return Ok(0);
    }
    doc.transaction(label, |doc| {
        doc.set_disp_data(label, updates)?;
        for group in &groups {
            let plan = plan_sew(doc, group, &[]);
            doc.set_disp_data(UNDO_SEW, plan.updates)?;
        }
        Ok::<(), EditError>(())
    })?;
    Ok(moved)
}

pub(crate) fn reshape(
    state: &mut AppState,
    label: &'static str,
    run: impl FnOnce(&mut [SmoothPart<'_>]) -> Vec<usize>,
) -> Option<(usize, usize)> {
    reshape_components(state, label, false, run)
}

fn reshape_components(
    state: &mut AppState,
    label: &'static str,
    separate: bool,
    run: impl FnOnce(&mut [SmoothPart<'_>]) -> Vec<usize>,
) -> Option<(usize, usize)> {
    let faces = targets(&state.doc);
    if !require(state, &faces) {
        return None;
    }
    match reshape_doc_components(&mut state.doc, &faces, label, separate, run) {
        Ok(moved) => Some((faces.len(), moved)),
        Err(e) => {
            failed(state, &e);
            None
        }
    }
}

pub fn subdivide(state: &mut AppState) -> usize {
    subdivide_with_shape(state, SubdivisionShape::Smooth, 1.0)
}

pub fn subdivide_with_shape(state: &mut AppState, shape: SubdivisionShape, amount: f64) -> usize {
    match reshape_components(state, UNDO_SUBDIVIDE, shape == SubdivisionShape::Sphere, |parts| match shape {
        SubdivisionShape::Smooth => subdivide_parts(parts, amount),
        SubdivisionShape::Sphere => sphere_parts(parts, amount),
    }) {
        Some((faces, moved)) if moved > 0 => {
            state.notify(Severity::Success, tf!("disp.subdivided", faces));
            moved
        }
        Some(_) => {
            state.notify(Severity::Info, t!("disp.subdivide_nothing"));
            0
        }
        None => 0,
    }
}

pub fn set_flag(state: &mut AppState, flag: i32, on: bool) -> usize {
    let faces = targets(&state.doc);
    if !require(state, &faces) {
        return 0;
    }
    match state.doc.set_disp_flags(&faces, flag, on) {
        Ok(n) => n,
        Err(e) => {
            failed(state, &e);
            0
        }
    }
}

pub fn flag_state(doc: &Document, faces: &[FaceRef], flag: i32) -> Option<bool> {
    let mut values =
        faces.iter().filter_map(|f| doc.face_side(*f).and_then(|s| s.disp.as_ref()).map(|d| d.flags & flag != 0));
    let first = values.next()?;
    values.all(|v| v == first).then_some(first)
}

pub const UNDO_SHADOWS: &str = "Displacement shadows";

pub fn set_cast_shadows_of(
    doc: &mut Document,
    faces: &[FaceRef],
    on: bool,
    label: &str,
) -> hp_editor::EditResult<usize> {
    let updates: Vec<(FaceRef, hp_vmf::DispInfo)> = faces
        .iter()
        .filter_map(|f| {
            let mut d = (**doc.face_side(*f)?.disp.as_ref()?).clone();
            d.set_casts_shadows(on).then_some((*f, d))
        })
        .collect();
    if updates.is_empty() {
        return Ok(0);
    }
    doc.set_disp_data(label, updates)
}

pub fn set_cast_shadows(state: &mut AppState, on: bool) -> usize {
    let faces = targets(&state.doc);
    if !require(state, &faces) {
        return 0;
    }
    match set_cast_shadows_of(&mut state.doc, &faces, on, UNDO_SHADOWS) {
        Ok(n) => n,
        Err(e) => {
            failed(state, &e);
            0
        }
    }
}

pub fn cast_shadows_state(doc: &Document, faces: &[FaceRef]) -> Option<bool> {
    let mut values =
        faces.iter().filter_map(|f| doc.face_side(*f).and_then(|s| s.disp.as_ref()).map(|d| d.casts_shadows()));
    let first = values.next()?;
    values.all(|v| v == first).then_some(first)
}

pub fn scale(state: &mut AppState, factor: f64) -> usize {
    let faces = targets(&state.doc);
    if !require(state, &faces) {
        return 0;
    }
    match state.doc.scale_disps(&faces, factor) {
        Ok(n) => n,
        Err(e) => {
            failed(state, &e);
            0
        }
    }
}

pub(crate) fn noise_range_ui(ui: &mut egui::Ui, state: &mut AppState) -> bool {
    let s = &mut state.settings.mesh;
    let mut changed = false;
    hp_app::widgets::form(ui, "hp_disp_noise_range", |ui| {
        ui.label(t!("disp.noise_min"));
        changed |= ui
            .add(egui::DragValue::new(&mut s.noise_min).speed(0.25).range(-1024.0..=1024.0).max_decimals(2))
            .on_hover_text(t!("disp.noise_tip"))
            .changed();
        ui.end_row();
        ui.label(t!("disp.noise_max"));
        changed |= ui
            .add(egui::DragValue::new(&mut s.noise_max).speed(0.25).range(-1024.0..=1024.0).max_decimals(2))
            .on_hover_text(t!("disp.noise_tip"))
            .changed();
        ui.end_row();
    });
    changed
}
