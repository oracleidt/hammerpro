use egui::{Color32, Modifiers, Pos2};
use glam::DVec3;
use hp_app::settings::SelectElement;
use hp_app::{AppState, ViewInfo};
use hp_editor::{EdgeRef, FaceRef, Id, ObjRef, PickOptions, SelectOp};
use hp_geom::{Aabb, Ray};
use hp_render::{Overlay, OverlayLine, OverlayPoint};

const PICK_PX: f32 = 9.0;

const MAX_POOL: usize = 64;

const SELECTED: Color32 = Color32::from_rgb(255, 214, 0);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Picked {
    Face(FaceRef),
    Edge(EdgeRef),
    Vertex(Id, DVec3),
}

fn color(c: Color32) -> [f32; 4] {
    let [r, g, b, a] = c.to_array();
    [f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, f32::from(a) / 255.0]
}

fn segment_px(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    let t = if len2 < 1e-6 { 0.0 } else { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) };
    (p - (a + ab * t)).length()
}

fn near_on_screen(view: &ViewInfo, ray: &Ray, bounds: &Aabb, pos: Pos2) -> bool {
    let mut rect: Option<egui::Rect> = None;
    for c in bounds.corners() {
        let Some(s) = view.world_to_screen(c) else {
            return bounds.expand(PICK_PX.into()).ray_intersect(ray).is_some();
        };
        rect = Some(rect.map_or(egui::Rect::from_two_pos(s, s), |r| r.union(egui::Rect::from_two_pos(s, s))));
    }
    rect.is_some_and(|r| r.expand(PICK_PX).contains(pos))
}

fn pool(state: &AppState, view: &ViewInfo, ray: &Ray, pos: Pos2, hits: &[hp_editor::Hit]) -> Vec<Id> {
    let mut ids: Vec<Id> = state.doc.selected_solid_ids();
    for h in hits.iter().take(8) {
        if let ObjRef::Solid(id) = h.obj {
            ids.push(id);
        }
    }
    let near: Vec<Id> = state
        .doc
        .scene()
        .solids()
        .iter()
        .filter(|(id, n)| n.geom.valid && state.doc.is_solid_visible(**id) && near_on_screen(view, ray, &n.bounds, pos))
        .map(|(id, _)| *id)
        .take(MAX_POOL)
        .collect();
    ids.extend(near);
    ids.sort_unstable();
    ids.dedup();
    ids.truncate(MAX_POOL);
    ids
}

pub(crate) fn pick(state: &AppState, view: &ViewInfo, pos: Pos2, element: SelectElement) -> Option<Picked> {
    if view.is_2d() || element == SelectElement::Object {
        return None;
    }
    let (o, d) = view.ray(pos);
    if !o.is_finite() || !d.is_finite() {
        return None;
    }
    let ray = Ray::new(o, d);
    let hits = state.view_pick_all(&ray, &PickOptions::default());
    if element == SelectElement::Face {
        return hits.iter().find_map(|h| match (h.obj, h.face) {
            (ObjRef::Solid(id), Some(side)) => Some(Picked::Face(FaceRef::new(id, side))),
            _ => None,
        });
    }
    let front = hits.first().map(|h| h.t);
    let visible = |p: DVec3| front.is_none_or(|t0| (p - o).dot(d) <= t0 + 4.0 + t0 * 0.01);
    let mut best: Option<(f32, f64, Picked)> = None;
    let mut offer = |px: f32, t: f64, what: Picked| {
        if px <= PICK_PX && best.as_ref().is_none_or(|b| px < b.0 - 0.5 || ((px - b.0).abs() <= 0.5 && t < b.1)) {
            best = Some((px, t, what));
        }
    };
    for id in pool(state, view, &ray, pos, &hits) {
        let Some(n) = state.doc.scene().solid(id) else { continue };
        if element == SelectElement::Edge {
            for [a, b] in n.geom.edges() {
                let (Some(sa), Some(sb)) = (view.world_to_screen(a), view.world_to_screen(b)) else { continue };
                let mid = (a + b) * 0.5;
                if visible(mid) {
                    offer(segment_px(pos, sa, sb), (mid - o).dot(d), Picked::Edge(EdgeRef::new(id, a, b)));
                }
            }
        } else {
            for p in n.geom.vertices() {
                let Some(s) = view.world_to_screen(p) else { continue };
                if visible(p) {
                    offer(s.distance(pos), (p - o).dot(d), Picked::Vertex(id, p));
                }
            }
        }
    }
    best.map(|b| b.2)
}

pub(crate) fn click(state: &mut AppState, view: &ViewInfo, pos: Pos2, mods: Modifiers) -> bool {
    let element = state.settings.selection.element;
    if element == SelectElement::Object || view.is_2d() {
        return false;
    }
    let add = mods.shift || mods.ctrl || mods.command;
    let picked = pick(state, view, pos, element);
    match picked {
        Some(Picked::Face(face)) => {
            state.runtime.selection.clear_elements();
            state.doc.select_faces(&[face], if add { SelectOp::Toggle } else { SelectOp::Replace });
        }
        Some(Picked::Edge(edge)) => {
            let sel = &mut state.runtime.selection;
            sel.vertices.clear();
            if sel.element_revision != state.doc.revision() {
                sel.edges.clear();
            }
            match sel.edges.iter().position(|e| e.same_edge(&edge)) {
                Some(i) if add => {
                    sel.edges.remove(i);
                }
                Some(_) => sel.edges = vec![edge],
                None if add => sel.edges.push(edge),
                None => sel.edges = vec![edge],
            }
            take_solid(state, edge.solid, add);
        }
        Some(Picked::Vertex(id, p)) => {
            let sel = &mut state.runtime.selection;
            sel.edges.clear();
            if sel.element_revision != state.doc.revision() {
                sel.vertices.clear();
            }
            match sel.vertices.iter().position(|(i, q)| *i == id && q.distance(p) < 1e-3) {
                Some(i) if add => {
                    sel.vertices.remove(i);
                }
                Some(_) => sel.vertices = vec![(id, p)],
                None if add => sel.vertices.push((id, p)),
                None => sel.vertices = vec![(id, p)],
            }
            take_solid(state, id, add);
        }
        None => {
            if !add {
                state.runtime.selection.clear_elements();
                if element == SelectElement::Face {
                    state.doc.clear_face_selection();
                }
            }
        }
    }
    state.runtime.selection.element_revision = state.doc.revision();
    true
}

fn take_solid(state: &mut AppState, id: Id, add: bool) {
    let op = if add { SelectOp::Add } else { SelectOp::Replace };
    state.doc.select(&[ObjRef::Solid(id)], op);
}

pub(crate) fn push_overlay(state: &AppState, out: &mut Overlay) {
    let rev = state.doc.revision();
    let sel = &state.runtime.selection;
    let c = color(SELECTED);
    for e in sel.fresh_edges(rev) {
        out.lines_on_top.push(OverlayLine { a: e.a.as_vec3().to_array(), b: e.b.as_vec3().to_array(), color: c });
        out.lines.push(OverlayLine { a: e.a.as_vec3().to_array(), b: e.b.as_vec3().to_array(), color: c });
    }
    for (_, p) in sel.fresh_vertices(rev) {
        out.points.push(OverlayPoint { pos: p.as_vec3().to_array(), color: c, size_px: 9.0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_to_a_segment_on_screen() {
        let (a, b) = (Pos2::new(0.0, 0.0), Pos2::new(10.0, 0.0));
        assert_eq!(segment_px(Pos2::new(5.0, 3.0), a, b), 3.0);
        assert_eq!(segment_px(Pos2::new(-4.0, 3.0), a, b), 5.0);
        assert_eq!(segment_px(Pos2::new(1.0, 1.0), a, a), (2.0f32).sqrt());
    }

    #[test]
    fn colors_are_normalized() {
        assert_eq!(color(Color32::from_rgb(255, 0, 0)), [1.0, 0.0, 0.0, 1.0]);
    }
}
