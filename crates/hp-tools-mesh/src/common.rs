use egui::{Color32, Pos2};
use glam::DVec3;
use hp_app::{ViewInfo, ViewInput, ViewKind, t, tf};
use hp_editor::EditError;
use hp_geom::{GeomError, ViewAxis};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Gesture {
    Hover(Pos2),
    Click(Pos2),
    DragStart { origin: Pos2, pos: Pos2 },
    Drag(Pos2),
    DragEnd(Pos2),
    Away,
}

pub(crate) fn gesture(input: &ViewInput<'_>) -> Gesture {
    use egui::PointerButton::Primary;
    let r = input.response;
    let pos = input.pointer.or_else(|| r.interact_pointer_pos()).or_else(|| r.hover_pos());
    if r.drag_started_by(Primary) {
        let origin = r.ctx.input(|i| i.pointer.press_origin()).or(pos);
        return match (origin, pos) {
            (Some(origin), Some(pos)) => Gesture::DragStart { origin, pos },
            (Some(p), None) | (None, Some(p)) => Gesture::DragStart { origin: p, pos: p },
            (None, None) => Gesture::Away,
        };
    }
    if r.drag_stopped() {
        return match pos.or_else(|| r.ctx.input(|i| i.pointer.latest_pos())) {
            Some(p) => Gesture::DragEnd(p),
            None => Gesture::Away,
        };
    }
    if r.dragged_by(Primary) {
        return pos.map_or(Gesture::Away, Gesture::Drag);
    }
    if r.clicked_by(Primary) {
        return pos.map_or(Gesture::Away, Gesture::Click);
    }
    match input.pointer {
        Some(p) => Gesture::Hover(p),
        None => Gesture::Away,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DocKey {
    revision: u64,
    next_object_id: u32,
    next_side_id: u32,
    solids: usize,
    hash: u64,
}

impl DocKey {
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn same_selection(&self, other: &DocKey) -> bool {
        self.hash == other.hash
    }

    pub(crate) fn of(doc: &hp_editor::Document) -> Self {
        use std::hash::{Hash, Hasher};
        let mut h = std::hash::DefaultHasher::new();
        doc.path().hash(&mut h);
        let sel = doc.selection();
        sel.objects.len().hash(&mut h);
        for o in &sel.objects {
            o.hash(&mut h);
        }
        Self {
            revision: doc.revision(),
            next_object_id: doc.peek_next_object_id(),
            next_side_id: doc.peek_next_side_id(),
            solids: doc.scene().solids().len(),
            hash: h.finish(),
        }
    }
}

pub(crate) fn view_axis(kind: ViewKind) -> Option<ViewAxis> {
    match kind {
        ViewKind::Top => Some(ViewAxis::Top),
        ViewKind::Front => Some(ViewAxis::Front),
        ViewKind::Side => Some(ViewAxis::Side),
        ViewKind::Perspective => None,
    }
}

pub(crate) fn rgba(c: Color32) -> [f32; 4] {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    [f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, f32::from(a) / 255.0]
}

pub(crate) fn f3(p: DVec3) -> [f32; 3] {
    [p.x as f32, p.y as f32, p.z as f32]
}

pub(crate) fn line_on_top(out: &mut hp_render::Overlay, a: DVec3, b: DVec3, color: [f32; 4]) {
    out.lines_on_top.push(hp_render::OverlayLine { a: f3(a), b: f3(b), color });
}

pub(crate) fn point(out: &mut hp_render::Overlay, p: DVec3, color: [f32; 4], size_px: f32) {
    out.points.push(hp_render::OverlayPoint { pos: f3(p), color, size_px });
}

pub(crate) fn ray_plane(origin: DVec3, dir: DVec3, plane_point: DVec3, normal: DVec3) -> Option<DVec3> {
    let denom = dir.dot(normal);
    if !denom.is_finite() || denom.abs() < 1e-9 {
        return None;
    }
    let t = (plane_point - origin).dot(normal) / denom;
    if !t.is_finite() {
        return None;
    }
    Some(origin + dir * t)
}

pub(crate) fn ray_line_param(ray_origin: DVec3, ray_dir: DVec3, line_origin: DVec3, line_dir: DVec3) -> Option<f64> {
    let a = line_dir.dot(line_dir);
    let b = line_dir.dot(ray_dir);
    let c = ray_dir.dot(ray_dir);
    let w0 = line_origin - ray_origin;
    let d = line_dir.dot(w0);
    let e = ray_dir.dot(w0);
    let denom = a * c - b * b;
    if !denom.is_finite() || denom.abs() < 1e-9 * (a * c).max(1e-12) {
        return None;
    }
    let s = (b * e - c * d) / denom;
    s.is_finite().then_some(s)
}

pub(crate) fn world_per_point(view: &ViewInfo, p: DVec3) -> f64 {
    if view.is_2d() {
        return 1.0 / view.cam2d.zoom.max(1e-9);
    }
    let depth = (p - view.cam3d.pos).dot(view.cam3d.forward()).max(1.0);
    let h = f64::from(view.rect.height().max(1.0));
    2.0 * depth * (view.cam3d.fov_y_deg.to_radians() * 0.5).tan() / h
}

pub(crate) fn tangent_basis(n: DVec3) -> (DVec3, DVec3) {
    let n = n.try_normalize().unwrap_or(DVec3::Z);
    let helper = if n.z.abs() < 0.9 { DVec3::Z } else { DVec3::X };
    let u = n.cross(helper).try_normalize().unwrap_or(DVec3::X);
    let v = n.cross(u);
    (u, v)
}

pub(crate) fn warning_badge(ui: &mut egui::Ui, tooltip: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let c = rect.center();
        let points = vec![
            egui::pos2(c.x, rect.top() + 1.5),
            egui::pos2(rect.right() - 0.5, rect.bottom() - 1.5),
            egui::pos2(rect.left() + 0.5, rect.bottom() - 1.5),
        ];
        let painter = ui.painter();
        painter.add(egui::Shape::convex_polygon(points, hp_app::theme::accent(), egui::Stroke::NONE));
        painter.text(
            egui::pos2(c.x, c.y + 2.0),
            egui::Align2::CENTER_CENTER,
            "!",
            egui::FontId::proportional(11.0),
            hp_app::theme::on_accent(),
        );
    }
    response.on_hover_text(tooltip)
}

pub(crate) fn geom_error_text(e: &GeomError) -> String {
    match e {
        GeomError::TooFewPoints(_) | GeomError::DegenerateHull => t!("mesh.err.flat").to_owned(),
        GeomError::NoVolume => t!("mesh.err.no_volume").to_owned(),
        GeomError::TooFewFaces(n) => tf!("mesh.err.too_few_faces", n),
        GeomError::NotFinite | GeomError::Unbounded => t!("mesh.err.not_finite").to_owned(),
        GeomError::NotQuad(n) => tf!("disp.err.not_quad", n),
        GeomError::BadPower(p) => tf!("disp.err.bad_power", p),
        other => tf!("mesh.err.geometry", other),
    }
}

pub(crate) fn vertex_error_text(e: &crate::vertex::model::VertexError) -> String {
    use crate::vertex::model::VertexError;
    match e {
        VertexError::Geom(g) => geom_error_text(g),
        VertexError::Edit(e) => edit_error_text(e),
        VertexError::Concave(n) => tf!("mesh.err.concave", n),
        VertexError::NotFlat(n) => tf!("mesh.err.not_flat", n),
    }
}

pub(crate) fn edit_error_text(e: &EditError) -> String {
    match e {
        EditError::Geom(g) => geom_error_text(g),
        EditError::Invalid(_) => t!("mesh.err.invalid").to_owned(),
        EditError::NotFound(_) | EditError::FaceNotFound(..) => t!("mesh.err.not_found").to_owned(),
        EditError::TransactionOpen => t!("mesh.err.busy").to_owned(),
        other => tf!("mesh.err.geometry", other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ray_plane_and_line() {
        let hit = ray_plane(DVec3::new(0.0, 0.0, 10.0), DVec3::NEG_Z, DVec3::ZERO, DVec3::Z).expect("hit");
        assert!((hit - DVec3::ZERO).length() < 1e-12);
        assert!(ray_plane(DVec3::ZERO, DVec3::X, DVec3::ZERO, DVec3::Z).is_none());
        let s = ray_line_param(DVec3::new(5.0, 0.0, 100.0), DVec3::NEG_Z, DVec3::ZERO, DVec3::X).expect("param");
        assert!((s - 5.0).abs() < 1e-9);
        assert!(ray_line_param(DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::X).is_none());
    }

    #[test]
    fn basis_is_orthonormal() {
        for n in [DVec3::Z, DVec3::X, DVec3::new(1.0, 2.0, 3.0).normalize(), DVec3::NEG_Z] {
            let (u, v) = tangent_basis(n);
            assert!(u.dot(n).abs() < 1e-9 && v.dot(n).abs() < 1e-9 && u.dot(v).abs() < 1e-9);
            assert!((u.length() - 1.0).abs() < 1e-9 && (v.length() - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn colors() {
        assert_eq!(rgba(Color32::from_rgb(255, 0, 0)), [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(f3(DVec3::new(1.0, 2.0, 3.0)), [1.0, 2.0, 3.0]);
    }
}
