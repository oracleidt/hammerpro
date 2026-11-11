use egui::{Color32, Painter, Pos2, Shape, Stroke, pos2, vec2};
use hp_app::{Action, ToolId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    Tool(ToolId),
    ApplyTexture,
    ToProp,
    ToBrushes,
    SendOut,
    Eye,
    EyeCrossed,
    EyeCrossedFramed,
    Group,
    Ungroup,
    Carve,
    Hammer,
}

pub fn action_icon(action: Action) -> Option<Icon> {
    Some(match action {
        Action::ApplyCurrentTexture => Icon::ApplyTexture,
        Action::BrushesToProp => Icon::ToProp,
        Action::PropToBrushes => Icon::ToBrushes,
        Action::SendToBlender => Icon::SendOut,
        Action::HideSelected => Icon::EyeCrossed,
        Action::HideUnselected => Icon::EyeCrossedFramed,
        Action::UnhideAll => Icon::Eye,
        Action::Group => Icon::Group,
        Action::Ungroup => Icon::Ungroup,
        Action::Carve => Icon::Carve,
        Action::RunMap => Icon::Hammer,
        other => Icon::Tool(other.tool()?),
    })
}

struct Pen<'a> {
    painter: &'a Painter,
    c: Pos2,
    k: f32,
    color: Color32,
    faint: Color32,
    line: Stroke,
    thin: Stroke,
    heavy: Stroke,
}

type P = (f32, f32);

impl Pen<'_> {
    fn p(&self, (x, y): P) -> Pos2 {
        pos2(self.c.x + x * self.k, self.c.y + y * self.k)
    }

    fn pts(&self, v: &[P]) -> Vec<Pos2> {
        v.iter().map(|q| self.p(*q)).collect()
    }

    fn path(&self, v: &[P]) {
        self.painter.add(Shape::line(self.pts(v), self.line));
    }

    fn path_with(&self, v: &[P], stroke: Stroke) {
        self.painter.add(Shape::line(self.pts(v), stroke));
    }

    fn closed(&self, v: &[P]) {
        self.painter.add(Shape::closed_line(self.pts(v), self.line));
    }

    fn seg(&self, a: P, b: P, stroke: Stroke) {
        self.painter.line_segment([self.p(a), self.p(b)], stroke);
    }

    fn dashed(&self, v: &[P], stroke: Stroke) {
        let dash = 2.0 * self.k;
        let gap = 1.5 * self.k;
        self.painter.extend(Shape::dashed_line(&self.pts(v), stroke, dash, gap));
    }

    fn fill(&self, v: &[P], color: Color32) {
        self.painter.add(Shape::convex_polygon(self.pts(v), color, Stroke::NONE));
    }

    fn rect(&self, a: P, b: P) -> egui::Rect {
        hp_app::widgets::crisp_rect(self.painter, egui::Rect::from_two_pos(self.p(a), self.p(b)))
    }

    fn rect_stroke(&self, a: P, b: P, radius: f32, stroke: Stroke) {
        self.painter.rect_stroke(self.rect(a, b), radius * self.k, stroke, egui::StrokeKind::Inside);
    }

    fn rect_fill(&self, a: P, b: P, color: Color32) {
        self.painter.rect_filled(self.rect(a, b), 0.0, color);
    }

    fn circle(&self, c: P, r: f32, stroke: Stroke) {
        self.painter.circle_stroke(self.p(c), r * self.k, stroke);
    }

    fn dot(&self, c: P, r: f32) {
        self.painter.circle_filled(self.p(c), r * self.k, self.color);
    }

    fn handle(&self, c: P) {
        let h = 1.5;
        self.rect_fill((c.0 - h, c.1 - h), (c.0 + h, c.1 + h), self.color);
    }

    fn arc(&self, c: P, rx: f32, ry: f32, from_deg: f32, to_deg: f32) -> Vec<P> {
        let n = 20;
        (0..=n)
            .map(|i| {
                let a = (from_deg + (to_deg - from_deg) * i as f32 / n as f32).to_radians();
                (c.0 + rx * a.cos(), c.1 + ry * a.sin())
            })
            .collect()
    }

    fn checker(&self, a: P, cells: i32, cell: f32) {
        let ppp = self.painter.pixels_per_point();
        let px = ((cell * self.k * ppp).round().max(2.0)) / ppp;
        let origin = self.rect(a, a).min;
        let size = px * cells as f32;
        for i in 0..cells {
            for j in 0..cells {
                if (i + j) % 2 == 0 {
                    let r = egui::Rect::from_min_size(origin + vec2(i as f32 * px, j as f32 * px), vec2(px, px));
                    self.painter.rect_filled(r, 0.0, self.color);
                }
            }
        }
        let outer = egui::Rect::from_min_size(origin, vec2(size, size));
        self.painter.rect_stroke(outer, 0.0, self.thin, egui::StrokeKind::Inside);
    }
}

pub fn paint(painter: &Painter, center: Pos2, color: Color32, icon: Icon, scale: f32) {
    let c = hp_app::widgets::crisp_center(painter, center);
    let pen = Pen {
        painter,
        c,
        k: scale,
        color,
        faint: color.gamma_multiply(0.3),
        line: hp_app::widgets::crisp_stroke(painter, 1.3 * scale.max(0.75), color),
        thin: hp_app::widgets::crisp_stroke(painter, 1.0, color),
        heavy: hp_app::widgets::crisp_stroke(painter, 2.0 * scale.max(0.75), color),
    };
    match icon {
        Icon::Tool(tool) => paint_tool(&pen, tool),
        Icon::ApplyTexture => {
            pen.checker((-1.5, -1.5), 3, 3.0);
            pen.seg((-7.0, -7.0), (-2.5, -2.5), pen.line);
            pen.path(&[(-6.5, -2.5), (-2.5, -2.5), (-2.5, -6.5)]);
        }
        Icon::ToProp => {
            pen.rect_stroke((-8.0, -8.0), (-2.0, -2.0), 0.5, pen.line);
            barrel(&pen, (4.0, 1.5));
            pen.path(&[(-1.0, -5.0), (4.0, -5.0), (4.0, -1.5)]);
            pen.path(&[(2.0, -3.5), (4.0, -1.5), (6.0, -3.5)]);
        }
        Icon::ToBrushes => {
            barrel(&pen, (-4.0, -7.5));
            pen.rect_stroke((1.0, 1.5), (7.5, 8.0), 0.5, pen.line);
            pen.path(&[(-0.5, -5.0), (4.0, -5.0), (4.0, -0.5)]);
            pen.path(&[(2.0, -2.5), (4.0, -0.5), (6.0, -2.5)]);
        }
        Icon::SendOut => {
            pen.path(&[(-1.0, -7.0), (-7.0, -7.0), (-7.0, 7.0), (7.0, 7.0), (7.0, 1.0)]);
            pen.seg((-1.0, 1.0), (6.5, -6.5), pen.line);
            pen.path(&[(1.5, -7.0), (7.0, -7.0), (7.0, -1.5)]);
        }
        Icon::Eye | Icon::EyeCrossed | Icon::EyeCrossedFramed => {
            eye(&pen);
            if icon != Icon::Eye {
                pen.seg((-8.0, -8.0), (8.0, 8.0), pen.heavy);
            }
            if icon == Icon::EyeCrossedFramed {
                pen.rect_stroke((-12.0, -10.0), (12.0, 10.0), 2.0, pen.thin);
            }
        }
        Icon::Group | Icon::Ungroup => {
            let apart = if icon == Icon::Ungroup { 3.5 } else { 0.0 };
            let (ax, ay) = (-3.0 - apart, -2.0);
            let (bx, by) = (3.0 + apart, 2.0);
            pen.painter.rect_filled(pen.rect((ax - 4.0, ay - 4.0), (ax + 4.0, ay + 4.0)), 1.0, color);
            pen.rect_stroke((bx - 4.0, by - 4.0), (bx + 4.0, by + 4.0), 1.0, pen.line);
            if icon == Icon::Group {
                pen.painter.rect_stroke(
                    pen.rect((ax - 6.5, ay - 6.5), (bx + 6.5, by + 6.5)),
                    2.0,
                    pen.thin,
                    egui::StrokeKind::Outside,
                );
            }
        }
        Icon::Carve => {
            pen.fill(&[(-8.0, -2.0), (-1.0, -2.0), (-1.0, 8.0), (-8.0, 8.0)], pen.faint);
            pen.fill(&[(-1.0, 2.0), (4.0, 2.0), (4.0, 8.0), (-1.0, 8.0)], pen.faint);
            pen.closed(&[(-8.0, -2.0), (-1.0, -2.0), (-1.0, 2.0), (4.0, 2.0), (4.0, 8.0), (-8.0, 8.0)]);
            pen.dashed(&[(-1.0, -2.0), (-1.0, -8.0), (8.0, -8.0), (8.0, 2.0), (4.0, 2.0)], pen.thin);
        }
        Icon::Hammer => {
            let head = [(-1.5, -7.0), (2.0, -3.5), (-3.5, 2.0), (-7.0, -1.5)];
            let head: Vec<P> = head.iter().map(|(x, y)| (x + 3.5, y + 0.5)).collect();
            pen.fill(&head, color);
            pen.seg((1.0, -0.5), (7.5, 6.0), pen.heavy);
        }
    }
}

fn barrel(pen: &Pen, top: P) {
    let (x, y) = top;
    let (rx, ry, h) = (3.5, 1.3, 5.0);
    pen.painter.add(Shape::closed_line(pen.pts(&pen.arc((x, y), rx, ry, 0.0, 360.0)), pen.line));
    pen.seg((x - rx, y), (x - rx, y + h), pen.line);
    pen.seg((x + rx, y), (x + rx, y + h), pen.line);
    pen.path(&pen.arc((x, y + h), rx, ry, 0.0, 180.0));
}

fn eye(pen: &Pen) {
    let (w, h) = (9.0, 5.5);
    let lid = |sign: f32| -> Vec<P> {
        (0..=16)
            .map(|i| {
                let x = -w + 2.0 * w * i as f32 / 16.0;
                (x, sign * h * (1.0 - (x / w).powi(2)))
            })
            .collect()
    };
    pen.path(&lid(-1.0));
    pen.path(&lid(1.0));
    pen.circle((0.0, 0.0), 3.6, pen.line);
    pen.dot((0.0, 0.0), 1.8);
}

fn paint_tool(pen: &Pen, tool: ToolId) {
    match tool {
        ToolId::Select => {
            let (a, b, cc, t1, t2, e, d) =
                ((-4.0, -8.0), (-4.0, 6.0), (-0.5, 2.5), (2.0, 7.5), (4.0, 6.5), (1.5, 1.5), (6.0, 1.5));
            for tri in [[a, b, cc], [a, cc, e], [a, e, d]] {
                pen.fill(&tri, pen.faint);
            }
            pen.fill(&[cc, t1, t2, e], pen.faint);
            pen.closed(&[a, b, cc, t1, t2, e, d]);
        }
        ToolId::Camera => {
            pen.rect_stroke((-8.0, -4.0), (8.0, 6.0), 2.0, pen.line);
            pen.path(&[(-3.5, -4.0), (-2.0, -6.5), (2.0, -6.5), (3.5, -4.0)]);
            pen.circle((0.0, 1.0), 3.0, pen.line);
            pen.dot((5.5, -1.5), 0.9);
        }
        ToolId::Entity => {
            let arc = pen.arc((0.0, -2.0), 5.0, 5.0, 120.0, 420.0);
            pen.path(&arc);
            let (l, r) = (arc[0], arc[arc.len() - 1]);
            pen.path(&[l, (l.0, 4.5), (r.0, 4.5), r]);
            pen.seg((-1.8, 7.0), (1.8, 7.0), pen.line);
        }
        ToolId::Block => {
            pen.fill(&[(0.0, -8.0), (7.0, -4.5), (0.0, -1.0), (-7.0, -4.5)], pen.faint);
            pen.closed(&[(0.0, -8.0), (7.0, -4.5), (7.0, 4.5), (0.0, 8.0), (-7.0, 4.5), (-7.0, -4.5)]);
            pen.path(&[(-7.0, -4.5), (0.0, -1.0), (7.0, -4.5)]);
            pen.seg((0.0, -1.0), (0.0, 8.0), pen.line);
        }
        ToolId::Texture => pen.checker((-6.5, -6.5), 4, 3.25),
        ToolId::Decal => {
            pen.fill(&[(7.0, 2.0), (2.0, 2.0), (2.0, 7.0)], pen.faint);
            pen.closed(&[(-7.0, -7.0), (7.0, -7.0), (7.0, 2.0), (2.0, 7.0), (-7.0, 7.0)]);
            pen.path(&[(7.0, 2.0), (2.0, 2.0), (2.0, 7.0)]);
            pen.circle((-1.5, -1.5), 2.5, pen.thin);
        }
        ToolId::Overlay => {
            pen.fill(&[(0.0, -7.5), (8.0, -3.5), (0.0, 0.5), (-8.0, -3.5)], pen.faint);
            pen.closed(&[(0.0, -7.5), (8.0, -3.5), (0.0, 0.5), (-8.0, -3.5)]);
            pen.path(&[(-8.0, 0.5), (0.0, 4.5), (8.0, 0.5)]);
            pen.path(&[(-8.0, 4.0), (0.0, 8.0), (8.0, 4.0)]);
        }
        ToolId::Clip => {
            pen.fill(&[(-6.0, -6.0), (6.0, -6.0), (6.0, -2.0), (-6.0, 2.0)], pen.faint);
            pen.rect_stroke((-6.0, -6.0), (6.0, 6.0), 0.5, pen.line);
            pen.seg((-8.5, 2.83), (8.5, -2.83), pen.heavy);
        }
        ToolId::Slice => {
            for (x0, x1) in [(-8.0, -3.5), (-2.25, 2.25), (3.5, 8.0)] {
                pen.rect_stroke((x0, -6.0), (x1, 6.0), 0.5, pen.line);
            }
            pen.fill(&[(-2.25, -6.0), (2.25, -6.0), (2.25, 6.0), (-2.25, 6.0)], pen.faint);
        }
        ToolId::Extrude => {
            let face = [(0.0, 1.5), (8.0, 4.5), (0.0, 7.5), (-8.0, 4.5)];
            pen.fill(&face, pen.faint);
            pen.closed(&face);
            pen.seg((0.0, 4.5), (0.0, -7.0), pen.line);
            pen.path(&[(-3.5, -3.5), (0.0, -7.5), (3.5, -3.5)]);
        }
        ToolId::Bevel => {
            pen.fill(&[(1.0, -7.0), (7.0, -7.0), (7.0, -1.0)], pen.faint);
            pen.path(&[(1.0, -7.0), (-7.0, -7.0), (-7.0, 7.0), (7.0, 7.0), (7.0, -1.0)]);
            pen.seg((1.0, -7.0), (7.0, -1.0), pen.heavy);
            pen.dashed(&[(1.0, -7.0), (7.0, -7.0), (7.0, -1.0)], pen.thin);
        }
        ToolId::Vertex => {
            let quad = [(-6.0, -5.0), (5.0, -7.0), (6.0, 5.0), (-5.0, 6.0)];
            pen.painter.add(Shape::closed_line(pen.pts(&quad), pen.thin));
            for q in quad {
                pen.handle(q);
            }
        }
        ToolId::Path => {
            let (p0, c1, c2, p3) = ((-7.0, 5.0), (-1.0, -8.0), (1.0, 8.0), (7.0, -5.0));
            let bez = egui::epaint::CubicBezierShape::from_points_stroke(
                [pen.p(p0), pen.p(c1), pen.p(c2), pen.p(p3)],
                false,
                Color32::TRANSPARENT,
                pen.line,
            );
            pen.painter.add(bez);
            let h1 = (p0.0 + 0.6 * (c1.0 - p0.0), p0.1 + 0.6 * (c1.1 - p0.1));
            let h2 = (p3.0 + 0.6 * (c2.0 - p3.0), p3.1 + 0.6 * (c2.1 - p3.1));
            pen.seg(p0, h1, pen.thin);
            pen.seg(p3, h2, pen.thin);
            pen.circle(h1, 1.4, pen.thin);
            pen.circle(h2, 1.4, pen.thin);
            pen.handle(p0);
            pen.handle(p3);
        }
        ToolId::Displacement => {
            let (a, b, m, d, e) = ((-8.0, 6.0), (-4.0, -2.0), (-1.0, 1.0), (3.0, -6.0), (8.0, 6.0));
            for tri in [[a, b, m], [a, m, e], [m, d, e]] {
                pen.fill(&tri, pen.faint);
            }
            pen.closed(&[a, b, m, d, e]);
            pen.path_with(&[(-6.0, 2.0), (-3.0, 3.5), (0.5, 2.5), (4.0, 3.5), (6.5, 2.5)], pen.thin);
        }
        ToolId::Polygon => {
            let pts: Vec<P> = (0..5)
                .map(|i| {
                    let a = (-90.0 + 72.0 * i as f32).to_radians();
                    (7.5 * a.cos(), 0.8 + 7.5 * a.sin())
                })
                .collect();
            pen.fill(&pts, pen.faint);
            pen.closed(&pts);
        }
        ToolId::Curve => {
            let c = (0.0, 6.0);
            let outer = pen.arc(c, 8.0, 8.0, 180.0, 360.0);
            let inner = pen.arc(c, 4.0, 4.0, 180.0, 360.0);
            for i in 0..outer.len() - 1 {
                pen.fill(&[outer[i], outer[i + 1], inner[i + 1], inner[i]], pen.faint);
            }
            let mut shape = outer;
            shape.extend(inner.iter().rev());
            pen.closed(&shape);
            for deg in [225.0f32, 270.0, 315.0] {
                let a = deg.to_radians();
                pen.seg(
                    (c.0 + 4.0 * a.cos(), c.1 + 4.0 * a.sin()),
                    (c.0 + 8.0 * a.cos(), c.1 + 8.0 * a.sin()),
                    pen.thin,
                );
            }
        }
        ToolId::Sprinkle => {
            let (x, y) = (-1.5, -1.5);
            let star = [
                (x, y - 6.0),
                (x + 1.5, y - 1.5),
                (x + 6.0, y),
                (x + 1.5, y + 1.5),
                (x, y + 6.0),
                (x - 1.5, y + 1.5),
                (x - 6.0, y),
                (x - 1.5, y - 1.5),
            ];
            for i in (0..8).step_by(2) {
                pen.fill(&[(x, y), star[(i + 7) % 8], star[i], star[i + 1]], pen.color);
            }
            pen.dot((5.5, -5.5), 1.3);
            pen.dot((5.5, 4.5), 1.7);
            pen.dot((-6.0, 6.0), 1.2);
            pen.dot((1.5, 7.0), 1.0);
        }
        ToolId::Road => {
            pen.seg((-8.0, 8.0), (-2.5, -8.0), pen.line);
            pen.seg((8.0, 8.0), (2.5, -8.0), pen.line);
            pen.seg((0.0, 7.5), (0.0, 4.0), pen.heavy);
            pen.seg((0.0, 1.5), (0.0, -1.0), pen.heavy);
            pen.seg((0.0, -3.5), (0.0, -5.5), pen.line);
        }
        ToolId::Pipe => {
            let c = (2.0, 2.0);
            let mut outer = vec![(-7.0, -4.0)];
            outer.extend(pen.arc(c, 6.0, 6.0, -90.0, 0.0));
            outer.push((8.0, 6.5));
            let mut inner = vec![(-7.0, 1.0)];
            inner.extend(pen.arc(c, 1.0, 1.0, -90.0, 0.0));
            inner.push((3.0, 6.5));
            pen.path(&outer);
            pen.path(&inner);
            pen.painter.add(Shape::closed_line(pen.pts(&pen.arc((-7.0, -1.5), 1.3, 2.5, 0.0, 360.0)), pen.line));
            pen.painter.add(Shape::closed_line(pen.pts(&pen.arc((5.5, 6.5), 2.5, 1.2, 0.0, 360.0)), pen.line));
        }
        ToolId::Cordon => {
            let (x0, y0, x1, y1, l) = (-7.5, -6.5, 7.5, 6.5, 3.5);
            pen.path(&[(x0, y0 + l), (x0, y0), (x0 + l, y0)]);
            pen.path(&[(x1 - l, y0), (x1, y0), (x1, y0 + l)]);
            pen.path(&[(x1, y1 - l), (x1, y1), (x1 - l, y1)]);
            pen.path(&[(x0 + l, y1), (x0, y1), (x0, y1 - l)]);
            pen.rect_fill((-2.0, -2.0), (2.0, 2.0), pen.color);
        }
        ToolId::Physics => {
            pen.seg((-8.0, 7.5), (8.0, 7.5), pen.line);
            pen.painter.circle(pen.p((1.0, 1.0)), 4.0 * pen.k, pen.faint, pen.line);
            pen.seg((-1.5, -8.5), (-1.5, -5.0), pen.thin);
            pen.seg((1.0, -8.5), (1.0, -4.0), pen.thin);
            pen.seg((3.5, -8.5), (3.5, -5.0), pen.thin);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(icon: Icon, ppp: f32) -> Vec<egui::epaint::ClippedShape> {
        let scale = match icon {
            Icon::Tool(_) | Icon::ApplyTexture | Icon::ToProp | Icon::ToBrushes | Icon::SendOut | Icon::Eye => 1.0,
            _ => 0.8,
        };
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(ppp);
        let mut shapes = Vec::new();
        for _ in 0..2 {
            let out = ctx.run_ui(egui::RawInput::default(), |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(34.0, 32.0), egui::Sense::hover());
                paint(ui.painter(), rect.center(), Color32::WHITE, icon, scale);
            });
            shapes = out.shapes;
        }
        shapes
    }

    fn bounds(shapes: &[egui::epaint::ClippedShape]) -> egui::Rect {
        shapes.iter().fold(egui::Rect::NOTHING, |r, s| r.union(s.shape.visual_bounding_rect()))
    }

    #[test]
    fn every_icon_draws_inside_its_button_at_every_scale() {
        let mut icons: Vec<Icon> = ToolId::ALL.iter().map(|t| Icon::Tool(*t)).collect();
        icons.extend([
            Icon::ApplyTexture,
            Icon::ToProp,
            Icon::ToBrushes,
            Icon::SendOut,
            Icon::Eye,
            Icon::EyeCrossed,
            Icon::Group,
            Icon::Ungroup,
            Icon::Carve,
            Icon::Hammer,
        ]);
        for ppp in [1.0, 1.25, 1.5] {
            for icon in &icons {
                let shapes = draw(*icon, ppp);
                assert!(!shapes.is_empty(), "{icon:?} draws something");
                let b = bounds(&shapes);
                assert!(b.width() <= 20.0 && b.height() <= 20.0, "{icon:?} at {ppp}: {b:?}");
                assert!(b.width() >= 8.0 && b.height() >= 8.0, "{icon:?} at {ppp} is too small: {b:?}");
            }
        }
    }

    #[test]
    fn icons_differ_from_each_other() {
        let mut seen: Vec<(ToolId, String)> = Vec::new();
        for tool in ToolId::ALL {
            let text = format!("{:?}", draw(Icon::Tool(tool), 1.0).iter().map(|s| &s.shape).collect::<Vec<_>>());
            for (other, t) in &seen {
                assert_ne!(*t, text, "{tool:?} looks like {other:?}");
            }
            seen.push((tool, text));
        }
    }

    #[test]
    fn strip_actions_and_toolbar_buttons_have_drawn_icons() {
        for a in [
            Action::ApplyCurrentTexture,
            Action::BrushesToProp,
            Action::PropToBrushes,
            Action::SendToBlender,
            Action::Carve,
            Action::RunMap,
            Action::ToolTexture,
        ] {
            assert!(action_icon(a).is_some(), "{a:?}");
        }
        assert_eq!(action_icon(Action::ToolExtrude), Some(Icon::Tool(ToolId::Extrude)));
        assert_eq!(action_icon(Action::SaveMap), None);
    }
}
