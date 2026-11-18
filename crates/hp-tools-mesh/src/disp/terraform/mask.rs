use glam::DVec3;
use hp_app::state::AppState;
use hp_editor::{Document, EditResult, FaceRef};
use hp_kv::Block;
use hp_vmf::DispInfo;

pub const MASK_BLOCK: &str = "hp_terrain_mask";
pub(crate) const UNDO_MASK: &str = "Displacement mask";

pub fn has_mask(disp: &DispInfo) -> bool {
    disp.extra_blocks.iter().any(|b| b.name.eq_ignore_ascii_case(MASK_BLOCK))
}

pub fn mask_of(disp: &DispInfo) -> Option<Vec<f64>> {
    let block = disp.extra_blocks.iter().find(|b| b.name.eq_ignore_ascii_case(MASK_BLOCK))?;
    let n = disp.row_len();
    if n < 2 {
        return None;
    }
    let mut out = vec![0.0; n * n];
    for row in 0..n {
        let Some(text) = block.get(&format!("row{row}")) else { continue };
        for (col, word) in text.split_whitespace().take(n).enumerate() {
            let v = word.parse::<f64>().ok().filter(|v| v.is_finite()).unwrap_or(0.0).clamp(0.0, 1.0);
            if let Some(slot) = out.get_mut(row * n + col) {
                *slot = v;
            }
        }
    }
    out.iter().any(|v| *v > 0.0).then_some(out)
}

fn push_value(text: &mut String, v: f64) {
    let r = (v.clamp(0.0, 1.0) * 1000.0).round() / 1000.0;
    if r <= 0.0 {
        text.push('0');
    } else if r >= 1.0 {
        text.push('1');
    } else {
        let s = format!("{r:.3}");
        text.push_str(s.trim_end_matches('0'));
    }
}

pub fn set_mask(disp: &mut DispInfo, values: &[f64]) -> bool {
    let before = disp.extra_blocks.clone();
    disp.extra_blocks.retain(|b| !b.name.eq_ignore_ascii_case(MASK_BLOCK));
    let n = disp.row_len();
    if values.len() == n * n && values.iter().any(|v| v.is_finite() && *v >= 0.0005) {
        let mut block = Block::new(MASK_BLOCK);
        for row in 0..n {
            let mut text = String::with_capacity(n * 4);
            for col in 0..n {
                if col > 0 {
                    text.push(' ');
                }
                let v = values.get(row * n + col).copied().filter(|v| v.is_finite()).unwrap_or(0.0);
                push_value(&mut text, v);
            }
            block.props.push((format!("row{row}"), text));
        }
        disp.extra_blocks.push(block);
    }
    disp.extra_blocks != before
}

pub(crate) struct Snapshot {
    faces: Vec<(FaceRef, DispInfo, Vec<f64>)>,
}

impl Snapshot {
    pub(crate) fn is_empty(&self) -> bool {
        self.faces.is_empty()
    }
}

pub(crate) fn snapshot(doc: &Document, faces: &[FaceRef]) -> Snapshot {
    let mut out = Vec::new();
    for f in faces {
        let Some(disp) = doc.face_side(*f).and_then(|s| s.disp.as_deref()) else { continue };
        if !has_mask(disp) {
            continue;
        }
        if let Some(mask) = mask_of(disp)
            && !out.iter().any(|(g, _, _)| g == f)
        {
            out.push((*f, disp.clone(), mask));
        }
    }
    Snapshot { faces: out }
}

fn blend_vertex(cur: &mut DispInfo, old: &DispInfo, k: usize, m: f64) -> bool {
    let get = |d: &DispInfo| {
        let n = d.normals.get(k).copied().unwrap_or(DVec3::Z);
        let dist = d.distances.get(k).copied().unwrap_or(0.0);
        let off = d.offsets.get(k).copied().unwrap_or(DVec3::ZERO);
        let a = d.alphas.get(k).copied().unwrap_or(0.0);
        (n * dist, off, a)
    };
    let (v_old, off_old, a_old) = get(old);
    let (v_new, off_new, a_new) = get(cur);
    if v_old.distance_squared(v_new) < 1e-18
        && off_old.distance_squared(off_new) < 1e-18
        && (a_old - a_new).abs() < 1e-9
    {
        return false;
    }
    let v = v_new + (v_old - v_new) * m;
    let off = off_new + (off_old - off_new) * m;
    let a = a_new + (a_old - a_new) * m;
    let len = v.length();
    if let (Some(nrm), Some(dist)) = (cur.normals.get_mut(k), cur.distances.get_mut(k)) {
        if len > 1e-9 {
            *nrm = v / len;
            *dist = len;
        } else {
            *dist = 0.0;
        }
    }
    if let Some(o) = cur.offsets.get_mut(k) {
        *o = off;
    }
    if let Some(x) = cur.alphas.get_mut(k) {
        *x = a;
    }
    true
}

pub(crate) fn restore(doc: &mut Document, snap: &Snapshot, label: &str) -> EditResult<usize> {
    let mut updates = Vec::new();
    for (f, old, mask) in &snap.faces {
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let Some(cur) = node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()) else { continue };
        if cur.power != old.power {
            continue;
        }
        let count = cur.row_len() * cur.row_len();
        let mut d = cur.clone();
        d.normals.resize(count, DVec3::Z);
        d.distances.resize(count, 0.0);
        d.offsets.resize(count, DVec3::ZERO);
        d.alphas.resize(count, 0.0);
        let mut changed = false;
        for (k, m) in mask.iter().enumerate().take(count) {
            if *m > 0.0 {
                changed |= blend_vertex(&mut d, old, k, *m);
            }
        }
        if !changed {
            continue;
        }
        if let Some(poly) = node.geom.face_for_side(f.side) {
            hp_geom::update_disp_tags(&mut d, &poly.vertices);
        }
        updates.push((*f, d));
    }
    if updates.is_empty() {
        return Ok(0);
    }
    doc.set_disp_data(label, updates)
}

pub fn guard_doc<R>(
    doc: &mut Document,
    faces: &[FaceRef],
    label: &str,
    op: impl FnOnce(&mut Document) -> EditResult<R>,
) -> EditResult<R> {
    let snap = snapshot(doc, faces);
    if snap.is_empty() {
        return op(doc);
    }
    doc.transaction(label.to_owned(), |doc| {
        let r = op(doc)?;
        restore(doc, &snap, label)?;
        Ok(r)
    })
}

pub fn guard_state<R>(state: &mut AppState, label: &str, op: impl FnOnce(&mut AppState) -> R) -> R {
    let faces = super::super::commands::targets(&state.doc);
    guard_state_faces(state, &faces, label, op)
}

pub fn guard_state_faces<R>(
    state: &mut AppState,
    faces: &[FaceRef],
    label: &str,
    op: impl FnOnce(&mut AppState) -> R,
) -> R {
    let faces = faces.to_vec();
    let snap = snapshot(&state.doc, &faces);
    if snap.is_empty() {
        return op(state);
    }
    state.doc.begin(label.to_owned());
    let r = op(state);
    if let Err(e) = restore(&mut state.doc, &snap, label) {
        state.notify(hp_app::Severity::Warning, hp_app::tf!("disp.failed", crate::common::edit_error_text(&e)));
    }
    state.doc.commit();
    r
}

pub(crate) fn masked_faces(doc: &Document) -> usize {
    doc.disp_faces()
        .into_iter()
        .filter(|f| doc.face_side(*f).and_then(|s| s.disp.as_deref()).is_some_and(has_mask))
        .count()
}

pub fn edit_masks(doc: &mut Document, faces: &[FaceRef], label: &str, f: impl Fn(&mut [f64])) -> EditResult<usize> {
    let mut updates = Vec::new();
    for face in faces {
        let Some(disp) = doc.face_side(*face).and_then(|s| s.disp.as_deref()) else { continue };
        let n = disp.row_len() * disp.row_len();
        let mut values = mask_of(disp).unwrap_or_else(|| vec![0.0; n]);
        values.resize(n, 0.0);
        f(&mut values);
        let mut d = disp.clone();
        if set_mask(&mut d, &values) {
            updates.push((*face, d));
        }
    }
    if updates.is_empty() {
        return Ok(0);
    }
    let count = updates.len();
    doc.set_disp_data(label, updates)?;
    Ok(count)
}

pub fn clear_masks(doc: &mut Document, faces: &[FaceRef]) -> EditResult<usize> {
    edit_masks(doc, faces, UNDO_MASK, |v| v.iter_mut().for_each(|x| *x = 0.0))
}

pub fn invert_masks(doc: &mut Document, faces: &[FaceRef]) -> EditResult<usize> {
    edit_masks(doc, faces, UNDO_MASK, |v| v.iter_mut().for_each(|x| *x = 1.0 - x.clamp(0.0, 1.0)))
}

pub fn fill_masks(doc: &mut Document, faces: &[FaceRef]) -> EditResult<usize> {
    edit_masks(doc, faces, UNDO_MASK, |v| v.iter_mut().for_each(|x| *x = 1.0))
}
