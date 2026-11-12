use egui::epaint::{Mesh, PathShape, PathStroke};
use egui::{Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Close,
    Star,
    StarOutline,
    Folder,
    Trash,
    Filter,
    Refresh,
    Import,
    Plus,
    Image,
    Cursor,
}

fn line(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    painter.line_segment([a, b], stroke);
}

fn star_points(c: Pos2, outer: f32, inner: f32) -> Vec<Pos2> {
    (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { outer } else { inner };
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            pos2(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

pub fn paint_star(painter: &egui::Painter, rect: Rect, color: Color32) {
    let c = rect.center();
    let outer = rect.width().min(rect.height()) * 0.5;
    let pts = star_points(c, outer, outer * 0.45);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(c, color);
    for p in &pts {
        mesh.colored_vertex(*p, color);
    }
    for i in 0..10u32 {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % 10);
    }
    painter.add(Shape::mesh(mesh));
}

pub fn paint_icon(painter: &egui::Painter, rect: Rect, icon: Icon, color: Color32) {
    let s = rect.width().min(rect.height());
    let r = Rect::from_center_size(rect.center(), Vec2::splat(s * 0.8));
    let stroke = Stroke::new((s * 0.09).clamp(1.0, 2.0), color);
    let (l, t, rt, b) = (r.left(), r.top(), r.right(), r.bottom());
    let c = r.center();
    let w = r.width();
    match icon {
        Icon::Close => {
            let k = w * 0.32;
            line(painter, c + vec2(-k, -k), c + vec2(k, k), stroke);
            line(painter, c + vec2(-k, k), c + vec2(k, -k), stroke);
        }
        Icon::Plus => {
            let k = w * 0.38;
            line(painter, c + vec2(-k, 0.0), c + vec2(k, 0.0), stroke);
            line(painter, c + vec2(0.0, -k), c + vec2(0.0, k), stroke);
        }
        Icon::Star => paint_star(painter, r, color),
        Icon::StarOutline => {
            let pts = star_points(c, w * 0.5, w * 0.22);
            painter.add(Shape::Path(PathShape::closed_line(pts, PathStroke::new(stroke.width, color))));
        }
        Icon::Folder => {
            let top = t + w * 0.18;
            let pts = vec![
                pos2(l, top),
                pos2(l + w * 0.35, top),
                pos2(l + w * 0.45, top + w * 0.12),
                pos2(rt, top + w * 0.12),
                pos2(rt, b - w * 0.08),
                pos2(l, b - w * 0.08),
            ];
            painter.add(Shape::Path(PathShape::closed_line(pts, PathStroke::new(stroke.width, color))));
            line(painter, pos2(l, top + w * 0.28), pos2(rt, top + w * 0.28), stroke);
        }
        Icon::Trash => {
            let lid = t + w * 0.18;
            line(painter, pos2(l + w * 0.05, lid), pos2(rt - w * 0.05, lid), stroke);
            line(painter, pos2(c.x - w * 0.12, lid), pos2(c.x - w * 0.12, t + w * 0.04), stroke);
            line(painter, pos2(c.x - w * 0.12, t + w * 0.04), pos2(c.x + w * 0.12, t + w * 0.04), stroke);
            line(painter, pos2(c.x + w * 0.12, t + w * 0.04), pos2(c.x + w * 0.12, lid), stroke);
            let pts =
                vec![pos2(l + w * 0.16, lid), pos2(rt - w * 0.16, lid), pos2(rt - w * 0.22, b), pos2(l + w * 0.22, b)];
            painter.add(Shape::Path(PathShape::closed_line(pts, PathStroke::new(stroke.width, color))));
            line(painter, pos2(c.x, lid + w * 0.18), pos2(c.x, b - w * 0.16), stroke);
        }
        Icon::Filter => {
            let pts = vec![
                pos2(l, t + w * 0.08),
                pos2(rt, t + w * 0.08),
                pos2(c.x + w * 0.1, c.y + w * 0.05),
                pos2(c.x + w * 0.1, b),
                pos2(c.x - w * 0.1, b - w * 0.12),
                pos2(c.x - w * 0.1, c.y + w * 0.05),
            ];
            painter.add(Shape::Path(PathShape::closed_line(pts, PathStroke::new(stroke.width, color))));
        }
        Icon::Refresh => {
            let radius = w * 0.4;
            let n = 20;
            let start = 0.35_f32;
            let end = std::f32::consts::TAU - 0.35;
            let pts: Vec<Pos2> = (0..=n)
                .map(|i| {
                    let a = start + (end - start) * i as f32 / n as f32;
                    pos2(c.x + radius * a.cos(), c.y + radius * a.sin())
                })
                .collect();
            let tip = pts.last().copied().unwrap_or(c);
            painter.add(Shape::Path(PathShape::line(pts, PathStroke::new(stroke.width, color))));
            line(painter, tip, tip + vec2(-w * 0.05, -w * 0.25), stroke);
            line(painter, tip, tip + vec2(w * 0.22, -w * 0.1), stroke);
        }
        Icon::Import => {
            line(painter, pos2(c.x, t), pos2(c.x, c.y + w * 0.15), stroke);
            line(painter, pos2(c.x, c.y + w * 0.15), pos2(c.x - w * 0.2, c.y - w * 0.05), stroke);
            line(painter, pos2(c.x, c.y + w * 0.15), pos2(c.x + w * 0.2, c.y - w * 0.05), stroke);
            let tray = vec![pos2(l, c.y + w * 0.1), pos2(l, b), pos2(rt, b), pos2(rt, c.y + w * 0.1)];
            painter.add(Shape::Path(PathShape::line(tray, PathStroke::new(stroke.width, color))));
        }
        Icon::Image => {
            painter.rect_stroke(r.shrink(w * 0.04), 1.5, stroke, egui::StrokeKind::Middle);
            let ground = vec![
                pos2(l + w * 0.1, b - w * 0.15),
                pos2(l + w * 0.38, c.y),
                pos2(l + w * 0.58, b - w * 0.3),
                pos2(l + w * 0.72, c.y + w * 0.08),
                pos2(rt - w * 0.1, b - w * 0.15),
            ];
            painter.add(Shape::Path(PathShape::line(ground, PathStroke::new(stroke.width, color))));
            painter.circle_filled(pos2(rt - w * 0.27, t + w * 0.28), w * 0.08, color);
        }
        Icon::Cursor => {
            let pts = vec![
                pos2(l + w * 0.2, t),
                pos2(l + w * 0.2, b - w * 0.1),
                pos2(l + w * 0.42, c.y + w * 0.18),
                pos2(l + w * 0.6, b),
                pos2(l + w * 0.72, b - w * 0.08),
                pos2(l + w * 0.55, c.y + w * 0.12),
                pos2(rt - w * 0.05, c.y + w * 0.1),
            ];
            painter.add(Shape::Path(PathShape::closed_line(pts, PathStroke::new(stroke.width, color))));
        }
    }
}

pub fn icon_button(ui: &mut Ui, icon: Icon, tooltip: &str) -> Response {
    icon_toggle(ui, icon, false, tooltip)
}

pub fn icon_toggle(ui: &mut Ui, icon: Icon, selected: bool, tooltip: &str) -> Response {
    let size = Vec2::splat(ui.spacing().interact_size.y);
    let (rect, response) = ui.allocate_exact_size(size, Sense::CLICK);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), tooltip));
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&response, selected);
        if selected || response.hovered() {
            ui.painter().rect(
                rect,
                visuals.corner_radius,
                visuals.weak_bg_fill,
                visuals.bg_stroke,
                egui::StrokeKind::Inside,
            );
        }
        let color = if selected { visuals.fg_stroke.color } else { visuals.text_color() };
        paint_icon(ui.painter(), rect.shrink(3.0), icon, color);
    }
    response.on_hover_text(tooltip)
}

pub fn icon_text_button(ui: &mut Ui, icon: Icon, text: &str, enabled: bool) -> Response {
    let padding = ui.spacing().button_padding;
    let icon_size = egui::TextStyle::Button.resolve(ui.style()).size;
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    let size = vec2(
        padding.x * 2.0 + icon_size + 4.0 + galley.size().x,
        (galley.size().y + padding.y * 2.0).max(ui.spacing().interact_size.y),
    );
    let sense = if enabled { Sense::CLICK } else { Sense::hover() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    if ui.is_rect_visible(rect) {
        let visuals = if enabled { *ui.style().interact(&response) } else { ui.style().visuals.widgets.noninteractive };
        ui.painter().rect(
            rect,
            visuals.corner_radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let color = if enabled { visuals.text_color() } else { ui.visuals().weak_text_color() };
        let icon_rect = Rect::from_min_size(
            pos2(rect.left() + padding.x, rect.center().y - icon_size * 0.5),
            Vec2::splat(icon_size),
        );
        paint_icon(ui.painter(), icon_rect, icon, color);
        let text_pos = pos2(icon_rect.right() + 4.0, rect.center().y - galley.size().y * 0.5);
        ui.painter().galley(text_pos, galley, color);
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_paints() {
        let mut state = hp_app::testing::test_state();
        let icons = [
            Icon::Close,
            Icon::Star,
            Icon::StarOutline,
            Icon::Folder,
            Icon::Trash,
            Icon::Filter,
            Icon::Refresh,
            Icon::Import,
            Icon::Plus,
            Icon::Image,
            Icon::Cursor,
        ];
        hp_app::testing::run_ui(&mut state, |ui, _| {
            for (i, icon) in icons.iter().enumerate() {
                icon_toggle(ui, *icon, i % 2 == 0, "tip");
                icon_text_button(ui, *icon, "Text", i % 3 != 0);
            }
        });
        assert_eq!(star_points(Pos2::ZERO, 2.0, 1.0).len(), 10);
    }
}
