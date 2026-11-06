use glam::DVec3;
use hp_app::view::ViewInfo;
use hp_app::{AppState, ToolId};
use hp_render::{Overlay, OverlayLine, OverlayPoint, OverlayTri};

use super::OverlayLayer;
use crate::overlay::{pixel_world_size, thick_line_on_top};

pub const FACE_FILL: [f32; 4] = [0.1, 0.75, 1.0, 0.38];
pub const FACE_FILL_THROUGH: [f32; 4] = [0.1, 0.75, 1.0, 0.1];
pub const ELEMENT_LINE: [f32; 4] = [0.35, 0.95, 1.0, 1.0];
pub const VERTEX_CORE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
pub const MAX_FACES: usize = 3000;
pub const MAX_THICK_FACES: usize = 150;
const VERTEX_PX: f32 = 13.0;
const VERTEX_CORE_PX: f32 = 6.0;

pub(crate) fn layers() -> Vec<Box<dyn OverlayLayer>> {
    vec![Box::new(SelectionLayer)]
}

pub struct SelectionLayer;

fn f3(v: DVec3) -> [f32; 3] {
    v.as_vec3().to_array()
}

pub fn push_face(out: &mut Overlay, view: &ViewInfo, poly: &[DVec3], thick: bool) {
    push_face_with_opacity(out, view, poly, thick, [FACE_FILL[3], FACE_FILL_THROUGH[3]]);
}

fn push_face_with_opacity(out: &mut Overlay, view: &ViewInfo, poly: &[DVec3], thick: bool, opacity: [f32; 2]) {
    if poly.len() < 3 || poly.iter().any(|p| !p.is_finite()) {
        return;
    }
    let normal = (poly[1] - poly[0]).cross(poly[2] - poly[1]).normalize_or_zero();
    let center = poly.iter().copied().sum::<DVec3>() / poly.len() as f64;
    let lift =
        if view.is_2d() { DVec3::ZERO } else { normal * (pixel_world_size(view, center) * 1.5).clamp(0.05, 4.0) };
    let p0 = f3(poly[0] + lift);
    for w in poly[1..].windows(2) {
        let tri = [p0, f3(w[0] + lift), f3(w[1] + lift)];
        let fill = [FACE_FILL[0], FACE_FILL[1], FACE_FILL[2], opacity[0]];
        let through = [FACE_FILL_THROUGH[0], FACE_FILL_THROUGH[1], FACE_FILL_THROUGH[2], opacity[1]];
        if !view.is_2d() && fill[3] > 0.0 {
            out.triangles.push(OverlayTri { p: tri, color: fill });
        }
        let top = if view.is_2d() { fill } else { through };
        if top[3] > 0.0 {
            out.triangles_on_top.push(OverlayTri { p: tri, color: top });
        }
    }
    for i in 0..poly.len() {
        let (a, b) = (poly[i] + lift, poly[(i + 1) % poly.len()] + lift);
        if thick {
            thick_line_on_top(out, view, a, b, ELEMENT_LINE);
        } else {
            out.lines_on_top.push(OverlayLine { a: f3(a), b: f3(b), color: ELEMENT_LINE });
        }
    }
}

pub fn push_edge(out: &mut Overlay, view: &ViewInfo, a: DVec3, b: DVec3) {
    thick_line_on_top(out, view, a, b, ELEMENT_LINE);
}

pub fn push_vertex(out: &mut Overlay, p: DVec3) {
    if !p.is_finite() {
        return;
    }
    out.points.push(OverlayPoint { pos: f3(p), color: ELEMENT_LINE, size_px: VERTEX_PX });
    out.points.push(OverlayPoint { pos: f3(p), color: VERTEX_CORE, size_px: VERTEX_CORE_PX });
}

impl OverlayLayer for SelectionLayer {
    fn name(&self) -> &'static str {
        "element_selection"
    }

    fn add(&self, state: &AppState, view: &ViewInfo, out: &mut Overlay) {
        if state.tool != ToolId::Texture {
            let thick = state.doc.selection().faces.len() <= MAX_THICK_FACES;
            for face in state.doc.selection().faces.iter().take(MAX_FACES) {
                if let Some(poly) = state.doc.face_polygon(*face) {
                    if state.tool == ToolId::Displacement {
                        let has_disp =
                            state.doc.scene().solid(face.solid).is_some_and(|n| n.disp_mesh(face.side).is_some());
                        let opacity = if has_disp { 0.0 } else { state.settings.mesh.displacement_selection_opacity() };
                        push_face_with_opacity(out, view, poly, thick, [opacity, 0.0]);
                    } else {
                        push_face(out, view, poly, thick);
                    }
                }
            }
        }
        let rev = state.doc.revision();
        let sel = &state.runtime.selection;
        for e in sel.fresh_edges(rev) {
            push_edge(out, view, e.a, e.b);
        }
        if state.tool != ToolId::Vertex {
            for (_, p) in sel.fresh_vertices(rev) {
                push_vertex(out, *p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use egui::{Rect, pos2, vec2};
    use glam::DVec2;
    use hp_app::view::{Camera2d, Camera3d, ViewKind};
    use hp_editor::{EdgeRef, FaceRef, SelectOp};

    use super::*;

    fn view(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 0,
            kind,
            rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(801.0, 601.0)),
            cam3d: Camera3d { pos: DVec3::new(0.0, 0.0, 128.0), yaw_deg: 0.0, pitch_deg: 0.0, ..Camera3d::default() },
            cam2d: Camera2d { center: DVec2::ZERO, zoom: 2.0 },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    fn drawn(state: &AppState, kind: ViewKind) -> Overlay {
        let mut out = Overlay::default();
        SelectionLayer.add(state, &view(kind), &mut out);
        out
    }

    #[test]
    fn selected_faces_edges_and_vertices_are_drawn_in_every_view() {
        let mut state = hp_app::testing::test_state();
        assert!(drawn(&state, ViewKind::Perspective).is_empty());
        state.doc.select_faces(&[FaceRef::new(3, 5)], SelectOp::Replace);
        let face = drawn(&state, ViewKind::Perspective);
        assert!(!face.triangles.is_empty() && !face.triangles_on_top.is_empty() && !face.lines_on_top.is_empty());
        assert!(face.points.is_empty());
        let flat = drawn(&state, ViewKind::Top);
        assert!(flat.triangles.is_empty() && !flat.triangles_on_top.is_empty() && !flat.lines_on_top.is_empty());
        state.tool = ToolId::Texture;
        assert!(drawn(&state, ViewKind::Perspective).is_empty(), "the texture tool paints its own mask");
        state.tool = ToolId::Select;
        state.doc.select_faces(&[], SelectOp::Replace);
        let rev = state.doc.revision();
        state.runtime.selection.element_revision = rev;
        state.runtime.selection.edges = vec![EdgeRef::new(3, DVec3::ZERO, DVec3::new(64.0, 0.0, 0.0))];
        state.runtime.selection.vertices = vec![(3, DVec3::ZERO)];
        let both = drawn(&state, ViewKind::Perspective);
        assert_eq!(both.lines_on_top.len(), 5);
        assert_eq!(both.points.len(), 2);
        state.runtime.selection.element_revision = rev.wrapping_add(1);
        assert!(drawn(&state, ViewKind::Perspective).is_empty(), "stale picks are not drawn");
    }

    #[test]
    fn displacement_opacity_does_not_leave_a_second_flat_mask() {
        let mut state = hp_app::testing::test_state();
        let face = FaceRef::new(3, 5);
        state.tool = ToolId::Displacement;
        state.doc.select_faces(&[face], SelectOp::Replace);
        for opacity in [0.0, 0.237, 1.0] {
            state.settings.mesh.disp_selection_opacity = opacity;
            let out = drawn(&state, ViewKind::Perspective);
            assert!(!out.lines_on_top.is_empty());
            assert!(out.triangles_on_top.is_empty());
            if opacity == 0.0 {
                assert!(out.triangles.is_empty());
            } else {
                assert!(out.triangles.iter().all(|t| t.color[3] == opacity));
            }
        }
        state.doc.create_displacements(&[face], 2).unwrap();
        let out = drawn(&state, ViewKind::Perspective);
        assert!(out.triangles.is_empty() && out.triangles_on_top.is_empty());
        assert!(!out.lines_on_top.is_empty());
        state.settings.mesh.disp_selection_opacity = 0.0;
        state.tool = ToolId::Select;
        assert!(!drawn(&state, ViewKind::Perspective).triangles.is_empty());
    }
}
