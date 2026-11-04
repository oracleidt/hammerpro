use std::cell::RefCell;

use egui::{Button, Pos2, RichText, Ui};
use glam::DVec3;
use hp_app::state::AppState;
use hp_app::tool::ToolOption;
use hp_app::widgets::section;
use hp_app::{Action, Severity, ViewInfo, t, tf};
use hp_editor::{Document, FaceRef};
use hp_geom::{DispMesh, EdgeNet};
use hp_vmf::DispInfo;

use super::commands::reshape_doc;
use super::paint::pick_disp;
use super::seams::{SEAM_TOL, brush_faces};
use super::smooth::{SmoothPart, Surface, find_junctions, follow_junctions, write_back};
use super::{DisplacementTool, PanelAction, pick_faces};
use crate::common::{line_on_top, point, rgba};

pub(crate) const MAX_LOOP_FACES: usize = 1024;
pub(crate) const UNDO_SMOOTH_LOOP: &str = "Smooth displacement loop";
pub(crate) const UNDO_LOOP_HEIGHT: &str = "Set height of displacement loop";
const PICK_PX: f32 = 10.0;
const MAX_PASSES: u32 = 64;
const LOOP_COLOR: egui::Color32 = egui::Color32::from_rgb(90, 220, 255);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoopSel {
    pub face: FaceRef,
    pub base_a: DVec3,
    pub base_b: DVec3,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoopView {
    pub positions: Vec<DVec3>,
    pub closed: bool,
    pub faces: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoopEdit {
    Smooth { strength: f64, passes: u32 },
    Height(f64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopAlong {
    Row,
    Column,
}

type Work = Vec<(FaceRef, DispMesh, DispInfo)>;

fn gather(doc: &Document, face: FaceRef) -> Work {
    let mut faces = brush_faces(doc, face, DVec3::ZERO, f64::INFINITY, false, &[]);
    if faces.is_empty() {
        faces.push(face);
    }
    faces.truncate(MAX_LOOP_FACES);
    let mut out = Vec::with_capacity(faces.len());
    for f in faces {
        let Some(node) = doc.scene().solid(f.solid) else { continue };
        let (Some(mesh), Some(disp)) =
            (node.disp_mesh(f.side), node.solid.sides.get(f.side).and_then(|s| s.disp.as_deref()))
        else {
            continue;
        };
        out.push((f, (**mesh).clone(), disp.clone()));
    }
    out
}

fn parts_of(work: &mut Work) -> Vec<SmoothPart<'_>> {
    work.iter_mut().map(|(_, mesh, disp)| SmoothPart { mesh, disp, pinned: false }).collect()
}

struct Analysis {
    surface: Surface,
    path: Vec<usize>,
    closed: bool,
}

fn node_at(surface: &Surface, parts: &[SmoothPart<'_>], base: DVec3) -> Option<usize> {
    surface
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let (pi, k) = *n.members.first()?;
            let b = *parts.get(pi)?.mesh.base_positions.get(k)?;
            Some((i, b.distance(base)))
        })
        .filter(|(_, d)| *d <= SEAM_TOL * 2.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(i, _)| i)
}

fn grid_net(parts: &[SmoothPart<'_>], surface: &Surface) -> EdgeNet {
    let points: Vec<DVec3> = surface.nodes.iter().map(|n| n.pos).collect();
    let mut faces: Vec<Vec<usize>> = Vec::new();
    for (pi, part) in parts.iter().enumerate() {
        let n = part.mesh.row_len;
        if n < 2 || part.mesh.positions.len() != n * n || part.mesh.base_positions.len() != n * n {
            continue;
        }
        for i in 0..n - 1 {
            for j in 0..n - 1 {
                let k = i * n + j;
                let ring: Option<Vec<usize>> =
                    [k, k + 1, k + n + 1, k + n].iter().map(|c| surface.node(pi, *c)).collect();
                let Some(ring) = ring else { continue };
                let mut sorted = ring.clone();
                sorted.sort_unstable();
                sorted.dedup();
                if sorted.len() == 4 {
                    faces.push(ring);
                }
            }
        }
    }
    EdgeNet::from_faces(points, faces)
}

fn analyze(parts: &[SmoothPart<'_>], sel: &LoopSel) -> Option<Analysis> {
    let surface = Surface::build(parts, |m, _| m.face_normal, |_, _| 1.0, None);
    let a = node_at(&surface, parts, sel.base_a)?;
    let b = node_at(&surface, parts, sel.base_b)?;
    let net = grid_net(parts, &surface);
    let e = net.edge_between(a, b)?;
    let run = net.edge_loop(e, None);
    let path = net.run_path(&run.edges);
    Some(Analysis { surface, path, closed: run.closed })
}

pub fn loop_view(doc: &Document, sel: &LoopSel) -> Option<LoopView> {
    let mut work = gather(doc, sel.face);
    let faces = work.len();
    let parts = parts_of(&mut work);
    let a = analyze(&parts, sel)?;
    let positions = a.path.iter().filter_map(|n| a.surface.nodes.get(*n).map(|n| n.pos)).collect();
    Some(LoopView { positions, closed: a.closed, faces })
}

fn height_of(parts: &[SmoothPart<'_>], members: &[(usize, usize)], axis: DVec3) -> f64 {
    let Some(&(pi, k)) = members.first() else { return 0.0 };
    let Some(p) = parts.get(pi) else { return 0.0 };
    let (Some(n), Some(d)) = (p.disp.normals.get(k), p.disp.distances.get(k)) else { return 0.0 };
    let v = *n * *d;
    if v.is_finite() { v.dot(axis) } else { 0.0 }
}

pub(crate) fn edit_parts(parts: &mut [SmoothPart<'_>], sel: &LoopSel, edit: LoopEdit) -> Vec<usize> {
    let mut changed = vec![0usize; parts.len()];
    let Some(a) = analyze(parts, sel) else { return changed };
    let junctions = find_junctions(parts, &a.surface);
    let nodes = &a.surface.nodes;
    let mut new_pos: Vec<DVec3> = nodes.iter().map(|n| n.pos).collect();
    let len = a.path.len();
    match edit {
        LoopEdit::Smooth { strength, passes } => {
            for _ in 0..passes {
                let cur = new_pos.clone();
                for (i, &n) in a.path.iter().enumerate() {
                    if !a.closed && (i == 0 || i + 1 == len) {
                        continue;
                    }
                    let (Some(&p), Some(&q)) = (a.path.get((i + len - 1) % len), a.path.get((i + 1) % len)) else {
                        continue;
                    };
                    let (Some(node), Some(&pp), Some(&qq), Some(&here)) =
                        (nodes.get(n), cur.get(p), cur.get(q), cur.get(n))
                    else {
                        continue;
                    };
                    let target = (pp + qq) * 0.5;
                    let lift = (target - here).dot(node.axis) * strength;
                    if let Some(slot) = new_pos.get_mut(n) {
                        *slot = here + node.axis * lift;
                    }
                }
            }
        }
        LoopEdit::Height(h) => {
            for &n in &a.path {
                let Some(node) = nodes.get(n) else { continue };
                let now = height_of(parts, &node.members, node.axis);
                if let Some(slot) = new_pos.get_mut(n) {
                    *slot = node.pos + node.axis * (h - now);
                }
            }
        }
    }
    follow_junctions(&junctions, nodes, &mut new_pos);
    write_back(parts, &a.surface, &new_pos, &mut changed);
    changed
}

pub fn edit_loop(doc: &mut Document, sel: &LoopSel, edit: LoopEdit) -> Result<usize, String> {
    let faces: Vec<FaceRef> = gather(doc, sel.face).into_iter().map(|(f, _, _)| f).collect();
    if loop_view(doc, sel).is_none() {
        return Err("the loop is not on the displacements".to_owned());
    }
    let label = match edit {
        LoopEdit::Smooth { .. } => UNDO_SMOOTH_LOOP,
        LoopEdit::Height(_) => UNDO_LOOP_HEIGHT,
    };
    reshape_doc(doc, &faces, label, |parts| edit_parts(parts, sel, edit)).map_err(|e| e.to_string())
}

pub fn loop_from_vertex(
    doc: &Document,
    face: FaceRef,
    row: usize,
    col: usize,
    along: LoopAlong,
) -> Result<LoopSel, String> {
    let mesh =
        doc.scene().solid(face.solid).and_then(|n| n.disp_mesh(face.side)).ok_or("that face is not a displacement")?;
    let n = mesh.row_len;
    if row >= n || col >= n {
        return Err(format!("row and col go from 0 to {}", n.saturating_sub(1)));
    }
    let (r2, c2) = match along {
        LoopAlong::Row if col + 1 < n => (row, col + 1),
        LoopAlong::Row => (row, col.saturating_sub(1)),
        LoopAlong::Column if row + 1 < n => (row + 1, col),
        LoopAlong::Column => (row.saturating_sub(1), col),
    };
    let base = |r: usize, c: usize| mesh.base_positions.get(r * n + c).copied();
    match (base(row, col), base(r2, c2)) {
        (Some(base_a), Some(base_b)) => Ok(LoopSel { face, base_a, base_b }),
        _ => Err("the displacement has no vertex grid".to_owned()),
    }
}

fn loop_faces(state: &AppState, view: &ViewInfo, pos: Pos2) -> Vec<FaceRef> {
    if view.is_2d() {
        return pick_faces(state, view, pos);
    }
    let (o, d) = view.ray(pos);
    pick_disp(&state.doc, &hp_geom::Ray::new(o, d)).map(|h| h.face).into_iter().collect()
}

fn segment_px(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 > 0.0 { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    p.distance(a + ab * t)
}

pub(crate) fn pick_loop(doc: &Document, view: &ViewInfo, pos: Pos2, faces: &[FaceRef]) -> Option<LoopSel> {
    let mut best: Option<(f32, LoopSel)> = None;
    for f in faces {
        let Some(mesh) = doc.scene().solid(f.solid).and_then(|n| n.disp_mesh(f.side)) else { continue };
        let n = mesh.row_len;
        if n < 2 || mesh.positions.len() != n * n || mesh.base_positions.len() != n * n {
            continue;
        }
        for i in 0..n {
            for j in 0..n {
                let k = i * n + j;
                for k2 in [(j + 1 < n).then_some(k + 1), (i + 1 < n).then_some(k + n)].into_iter().flatten() {
                    let (Some(sa), Some(sb)) =
                        (view.world_to_screen(mesh.positions[k]), view.world_to_screen(mesh.positions[k2]))
                    else {
                        continue;
                    };
                    let d = segment_px(pos, sa, sb);
                    if d <= PICK_PX && best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((
                            d,
                            LoopSel { face: *f, base_a: mesh.base_positions[k], base_b: mesh.base_positions[k2] },
                        ));
                    }
                }
            }
        }
    }
    best.map(|(_, s)| s)
}

#[derive(Debug)]
pub(crate) struct LoopState {
    pub sel: Option<LoopSel>,
    pub height: f64,
    pub strength: f64,
    pub passes: u32,
    cache: RefCell<Option<(u64, LoopSel, Option<LoopView>)>>,
}

impl Default for LoopState {
    fn default() -> Self {
        Self { sel: None, height: 0.0, strength: 0.5, passes: 1, cache: RefCell::new(None) }
    }
}

impl LoopState {
    pub fn view(&self, doc: &Document) -> Option<LoopView> {
        let sel = self.sel?;
        let rev = doc.revision();
        if let Some((r, s, v)) = self.cache.borrow().as_ref()
            && *r == rev
            && *s == sel
        {
            return v.clone();
        }
        let v = loop_view(doc, &sel);
        *self.cache.borrow_mut() = Some((rev, sel, v.clone()));
        v
    }

    pub fn set_option(&mut self, key: &str, value: &str) -> Option<bool> {
        let number = value.trim().parse::<f64>().ok().filter(|v| v.is_finite());
        Some(match key {
            "loop_height" => number.filter(|v| (-16384.0..=16384.0).contains(v)).map(|v| self.height = v).is_some(),
            "loop_strength" => number.filter(|v| (0.0..=1.0).contains(v)).map(|v| self.strength = v).is_some(),
            "loop_passes" => number
                .filter(|v| (1.0..=f64::from(MAX_PASSES)).contains(v) && v.fract() == 0.0)
                .map(|v| self.passes = v as u32)
                .is_some(),
            _ => return None,
        })
    }

    pub fn option_values(&self) -> Vec<ToolOption> {
        vec![
            ToolOption::new(
                "loop_height",
                format!("{}", self.height),
                "units from -16384 to 16384: the height Set height of loop puts the loop vertices at, above the face",
            ),
            ToolOption::new(
                "loop_strength",
                format!("{}", self.strength),
                "0 to 1: how far Smooth loop moves each vertex toward the average of its two loop neighbours per pass",
            ),
            ToolOption::new("loop_passes", self.passes.to_string(), "1 to 64: passes of Smooth loop"),
        ]
    }
}

impl DisplacementTool {
    pub(super) fn loop_click(&mut self, state: &mut AppState, view: &ViewInfo, pos: Pos2) -> bool {
        let faces = loop_faces(state, view, pos);
        let faces: Vec<FaceRef> = faces
            .into_iter()
            .filter(|f| state.doc.scene().solid(f.solid).is_some_and(|n| n.disp_mesh(f.side).is_some()))
            .collect();
        let Some(sel) = pick_loop(&state.doc, view, pos, &faces) else {
            state.log(Severity::Info, t!("disp.loop.no_edge").to_owned());
            return false;
        };
        self.loops.sel = Some(sel);
        if let Some(v) = self.loops.view(&state.doc) {
            let text = if v.closed {
                tf!("disp.loop.done_closed", v.positions.len(), v.faces)
            } else {
                tf!("disp.loop.done", v.positions.len(), v.faces)
            };
            state.log(Severity::Info, text);
        }
        true
    }

    pub(super) fn take_loop_request(&mut self, state: &mut AppState) {
        let Some(r) = state.runtime.mesh.disp_loop_request.take() else { return };
        self.loops.sel = Some(LoopSel { face: FaceRef::new(r.solid, r.side), base_a: r.base_a, base_b: r.base_b });
    }

    pub(super) fn loop_action(&mut self, state: &mut AppState, action: Action) -> bool {
        match action {
            Action::DispSmoothLoop => {
                self.smooth_loop(state);
                true
            }
            Action::DispLoopHeight => {
                self.set_loop_height(state);
                true
            }
            _ => false,
        }
    }

    fn need_loop(&mut self, state: &mut AppState) -> Option<LoopSel> {
        self.take_loop_request(state);
        if self.loops.sel.is_some() && self.loops.view(&state.doc).is_some() {
            return self.loops.sel;
        }
        self.warn(state, t!("disp.loop.need").to_owned());
        None
    }

    pub(crate) fn smooth_loop(&mut self, state: &mut AppState) -> usize {
        let Some(sel) = self.need_loop(state) else { return 0 };
        let edit = LoopEdit::Smooth { strength: self.loops.strength, passes: self.loops.passes };
        self.run_loop_edit(state, &sel, edit)
    }

    pub(crate) fn set_loop_height(&mut self, state: &mut AppState) -> usize {
        let Some(sel) = self.need_loop(state) else { return 0 };
        self.run_loop_edit(state, &sel, LoopEdit::Height(self.loops.height))
    }

    fn run_loop_edit(&mut self, state: &mut AppState, sel: &LoopSel, edit: LoopEdit) -> usize {
        match edit_loop(&mut state.doc, sel, edit) {
            Ok(0) => {
                state.notify(Severity::Info, t!("disp.loop.nothing"));
                0
            }
            Ok(n) => {
                state.notify(Severity::Success, tf!("disp.loop.edited", n));
                n
            }
            Err(e) => {
                self.warn(state, tf!("disp.failed", e));
                0
            }
        }
    }

    pub(super) fn loop_overlay(&self, state: &AppState, out: &mut hp_render::Overlay) {
        let Some(v) = self.loops.view(&state.doc) else { return };
        let color = rgba(LOOP_COLOR);
        for w in v.positions.windows(2) {
            if let [a, b] = w {
                line_on_top(out, *a, *b, color);
            }
        }
        if v.closed
            && v.positions.len() > 2
            && let (Some(a), Some(b)) = (v.positions.first(), v.positions.last())
        {
            line_on_top(out, *a, *b, color);
        }
        for p in &v.positions {
            point(out, *p, color, 6.0);
        }
    }

    pub(super) fn loop_status(&self, state: &AppState) -> Option<String> {
        let v = self.loops.view(&state.doc)?;
        Some(if v.closed {
            tf!("disp.loop.status_closed", v.positions.len(), v.faces)
        } else {
            tf!("disp.loop.status", v.positions.len(), v.faces)
        })
    }

    pub(super) fn loop_ui(&mut self, ui: &mut Ui, state: &mut AppState, actions: &mut Vec<PanelAction>) {
        section(ui, t!("disp.loop.title"));
        let view = self.loops.view(&state.doc);
        let line = match &view {
            Some(v) if v.closed => tf!("disp.loop.status_closed", v.positions.len(), v.faces),
            Some(v) => tf!("disp.loop.status", v.positions.len(), v.faces),
            None => t!("disp.loop.hint").to_owned(),
        };
        ui.label(RichText::new(line).weak());
        let any = view.is_some();
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("disp.loop.strength")).on_hover_text(t!("disp.loop.strength_tip"));
            ui.add(egui::DragValue::new(&mut self.loops.strength).speed(0.01).range(0.0..=1.0).max_decimals(2))
                .on_hover_text(t!("disp.loop.strength_tip"));
            ui.label(t!("disp.loop.passes")).on_hover_text(t!("disp.loop.passes_tip"));
            ui.add(egui::DragValue::new(&mut self.loops.passes).speed(0.1).range(1..=MAX_PASSES))
                .on_hover_text(t!("disp.loop.passes_tip"));
            if ui
                .add_enabled(any, Button::new(Action::DispSmoothLoop.label()))
                .on_hover_text(t!("disp.loop.smooth_tip"))
                .on_disabled_hover_text(t!("disp.loop.smooth_tip"))
                .clicked()
            {
                actions.push(PanelAction::LoopSmooth);
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(t!("disp.loop.height")).on_hover_text(t!("disp.loop.height_tip"));
            ui.add(egui::DragValue::new(&mut self.loops.height).speed(1.0).range(-16384.0..=16384.0).max_decimals(2))
                .on_hover_text(t!("disp.loop.height_tip"));
            if ui
                .add_enabled(any, Button::new(Action::DispLoopHeight.label()))
                .on_hover_text(t!("disp.loop.height_tip"))
                .on_disabled_hover_text(t!("disp.loop.height_tip"))
                .clicked()
            {
                actions.push(PanelAction::LoopHeight);
            }
            if ui.add_enabled(any, Button::new(t!("disp.loop.clear"))).clicked() {
                actions.push(PanelAction::LoopClear);
            }
        });
    }
}

pub const TR: hp_app::i18n::TrTable = &[
    ("disp.loop.title", "Edge loop", "Boucle d'arêtes"),
    (
        "disp.loop.hint",
        "Alt+click a grid line of a displacement (3D or 2D view) to select its row or column, across sewn neighbours",
        "Alt+clic sur une ligne de la grille d'un displacement (vue 3D ou 2D) pour sélectionner sa rangée ou sa colonne, à travers les voisins soudés",
    ),
    (
        "disp.loop.status",
        "Loop: {0} vertices across {1} displacement(s)",
        "Boucle : {0} sommets sur {1} displacement(s)",
    ),
    (
        "disp.loop.status_closed",
        "Closed loop: {0} vertices across {1} displacement(s)",
        "Boucle fermée : {0} sommets sur {1} displacement(s)",
    ),
    (
        "disp.loop.done",
        "Loop selected: {0} vertices across {1} displacement(s)",
        "Boucle sélectionnée : {0} sommets sur {1} displacement(s)",
    ),
    (
        "disp.loop.done_closed",
        "Closed loop selected: {0} vertices across {1} displacement(s)",
        "Boucle fermée sélectionnée : {0} sommets sur {1} displacement(s)",
    ),
    ("disp.loop.no_edge", "No grid line under the cursor.", "Aucune ligne de grille sous le curseur."),
    (
        "disp.loop.need",
        "Select a loop first: Alt+click a grid line of a displacement.",
        "Sélectionnez d'abord une boucle : Alt+clic sur une ligne de la grille d'un displacement.",
    ),
    ("disp.loop.nothing", "The loop already has that shape.", "La boucle a déjà cette forme."),
    ("disp.loop.edited", "{0} vertices of the loop changed.", "{0} sommets de la boucle modifiés."),
    ("disp.loop.strength", "Strength", "Force"),
    (
        "disp.loop.strength_tip",
        "0 to 1: how far each vertex of the loop moves toward the average of its two neighbours on the loop, per pass (along the face normal, the ends of an open loop stay)",
        "0 à 1 : de combien chaque sommet de la boucle va vers la moyenne de ses deux voisins sur la boucle, à chaque passe (le long de la normale de la face, les extrémités d'une boucle ouverte restent)",
    ),
    ("disp.loop.passes", "Passes", "Passes"),
    ("disp.loop.passes_tip", "How many times the smoothing is repeated", "Combien de fois le lissage est répété"),
    (
        "disp.loop.smooth_tip",
        "Smooth the selected loop along its own line, sewn neighbours follow",
        "Lisse la boucle sélectionnée le long de sa ligne, les voisins soudés suivent",
    ),
    ("disp.loop.height", "Height", "Hauteur"),
    (
        "disp.loop.height_tip",
        "Set the height above the face of every vertex of the selected loop to this value (like Raise to, on the loop only)",
        "Fixe la hauteur au-dessus de la face de chaque sommet de la boucle sélectionnée à cette valeur (comme Élever à, sur la boucle seulement)",
    ),
    ("disp.loop.clear", "Clear loop", "Effacer la boucle"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translations_are_clean() {
        assert!(hp_app::i18n::lint_table(TR).is_empty());
        for (k, _, _) in TR {
            assert!(k.starts_with("disp.loop."));
        }
    }
}
