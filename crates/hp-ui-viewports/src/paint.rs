use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2, vec2};
use glam::DVec3;
use hp_app::theme;
use hp_app::view::ViewInfo;

pub const TEXT: Color32 = Color32::from_gray(228);
pub const TEXT_WEAK: Color32 = Color32::from_gray(160);
pub const PILL_BG: Color32 = Color32::from_rgba_premultiplied(10, 10, 10, 190);

pub fn overlay_color(c: [f32; 4]) -> Color32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0) as u8;
    Color32::from_rgb(b(c[0]), b(c[1]), b(c[2]))
}

pub fn small_font() -> FontId {
    FontId::proportional(11.5)
}

pub fn text_pill(painter: &Painter, pos: Pos2, anchor: Align2, text: String, color: Color32) -> Rect {
    let galley = painter.layout_no_wrap(text, small_font(), color);
    let rect = anchor.anchor_size(pos, galley.size() + vec2(10.0, 4.0));
    painter.rect_filled(rect, 4, PILL_BG);
    painter.galley(rect.min + vec2(5.0, 2.0), galley, color);
    rect
}

pub fn walk_legend(painter: &Painter, corner: Pos2, mode: hp_app::settings::DispWalkMode, angle: f32) -> Rect {
    use hp_app::settings::DispWalkMode;
    let limit = f64::from(hp_app::runtime::render::walk_limit(angle)) / 100.0;
    let swatch_of = |c: hp_render::WalkClass| {
        let [r, g, b] = c.srgb();
        Color32::from_rgb(r, g, b)
    };
    let rows: Vec<(Color32, String)> = match mode {
        DispWalkMode::Source => hp_render::WalkClass::ALL
            .iter()
            .map(|c| {
                let label = match c {
                    hp_render::WalkClass::Buildable => hp_app::t!("viewport.walk_buildable"),
                    hp_render::WalkClass::Walkable => hp_app::t!("viewport.walk_walkable"),
                    _ => hp_app::t!("viewport.walk_steep"),
                };
                (swatch_of(*c), label.to_owned())
            })
            .collect(),
        DispWalkMode::Limit => vec![
            (swatch_of(hp_render::WalkClass::Buildable), hp_app::tf!("viewport.walk_below", format!("{limit:.2}"))),
            (swatch_of(hp_render::WalkClass::Steep), hp_app::tf!("viewport.walk_above", format!("{limit:.2}"))),
        ],
        DispWalkMode::Gradient => hp_render::WalkClass::GRADIENT
            .iter()
            .enumerate()
            .map(|(band, c)| {
                let (lo, hi) = hp_geom::slope_band_range(band, limit);
                let text = if band == hp_render::WalkClass::GRADIENT.len() - 1 {
                    hp_app::t!("viewport.walk_overhang").to_owned()
                } else {
                    hp_app::tf!("viewport.walk_band", format!("{lo:.0}"), format!("{hi:.0}"))
                };
                (swatch_of(*c), text)
            })
            .collect(),
    };
    let title = painter.layout_no_wrap(hp_app::t!("viewport.walk_title").to_owned(), small_font(), TEXT_WEAK);
    let galleys: Vec<_> =
        rows.iter().map(|(_, label)| painter.layout_no_wrap(label.clone(), small_font(), TEXT)).collect();
    let line = 15.0;
    let swatch = 10.0;
    let width = galleys.iter().map(|g| g.size().x + swatch + 6.0).fold(title.size().x, f32::max) + 12.0;
    let height = line * (rows.len() as f32 + 1.0) + 6.0;
    let rect = Align2::RIGHT_BOTTOM.anchor_size(corner, vec2(width, height));
    painter.rect_filled(rect, 4, PILL_BG);
    let mut y = rect.top() + 3.0;
    painter.galley(pos2(rect.left() + 6.0, y), title, TEXT_WEAK);
    for ((color, _), galley) in rows.iter().zip(galleys) {
        y += line;
        let sw = Rect::from_min_size(pos2(rect.left() + 6.0, y + (line - swatch) * 0.5 - 1.0), vec2(swatch, swatch));
        painter.rect_filled(sw, 2, *color);
        painter.galley(pos2(sw.right() + 6.0, y), galley, TEXT);
    }
    rect
}

pub fn axis_screen_dir(view: &ViewInfo, axis: DVec3) -> Vec2 {
    let (right, up) = crate::overlay::screen_axes(view);
    vec2(axis.dot(right) as f32, -axis.dot(up) as f32)
}

pub fn axis_gizmo(painter: &Painter, origin: Pos2, view: &ViewInfo) {
    const LEN: f32 = 17.0;
    let mut axes = [(DVec3::X, theme::AXIS_X, "X"), (DVec3::Y, theme::AXIS_Y, "Y"), (DVec3::Z, theme::AXIS_Z, "Z")];
    let forward = if view.is_2d() { -view.kind.toward_viewer() } else { view.cam3d.forward() };
    axes.sort_by(|a, b| b.0.dot(forward).total_cmp(&a.0.dot(forward)));
    painter.circle_filled(origin, LEN + 7.0, Color32::from_black_alpha(80));
    for (axis, color, name) in axes {
        let d = axis_screen_dir(view, axis);
        if d.length() < 0.05 {
            painter.circle_filled(origin, 2.5, color);
            continue;
        }
        let tip = origin + d * LEN;
        painter.line_segment([origin, tip], Stroke::new(2.0, color));
        painter.text(origin + d * (LEN + 6.0), Align2::CENTER_CENTER, name, FontId::proportional(10.0), color);
    }
}

pub fn placeholder(painter: &Painter, rect: Rect, background: Color32, title: &str, subtitle: &str) {
    painter.rect_filled(rect, 0, background);
    let c = rect.center();
    painter.text(c - vec2(0.0, 9.0), Align2::CENTER_CENTER, title, FontId::proportional(16.0), TEXT_WEAK);
    painter.text(c + vec2(0.0, 12.0), Align2::CENTER_CENTER, subtitle, small_font(), TEXT_WEAK.gamma_multiply(0.7));
}

pub fn maximize_icon(painter: &Painter, rect: Rect, maximized: bool, color: Color32) {
    let stroke = Stroke::new(1.2, color);
    let r = rect.shrink(4.5);
    if maximized {
        let back = Rect::from_min_max(r.min + vec2(2.5, 0.0), r.max - vec2(0.0, 2.5));
        let front = Rect::from_min_max(r.min + vec2(0.0, 2.5), r.max - vec2(2.5, 0.0));
        painter.rect_stroke(back, 1, stroke, StrokeKind::Middle);
        painter.rect_filled(front, 1, PILL_BG);
        painter.rect_stroke(front, 1, stroke, StrokeKind::Middle);
    } else {
        painter.rect_stroke(r, 1, stroke, StrokeKind::Middle);
        painter.hline(r.x_range(), r.top() + 1.0, Stroke::new(2.0, color));
    }
}

pub fn chevron(painter: &Painter, center: Pos2, color: Color32) {
    let pts = vec![
        pos2(center.x - 3.5, center.y - 1.5),
        pos2(center.x + 3.5, center.y - 1.5),
        pos2(center.x, center.y + 2.5),
    ];
    painter.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

pub fn crosshair(painter: &Painter, center: Pos2) {
    let s = Stroke::new(1.5, Color32::from_white_alpha(200));
    let shadow = Stroke::new(3.0, Color32::from_black_alpha(120));
    for stroke in [shadow, s] {
        painter.line_segment([center - vec2(8.0, 0.0), center - vec2(3.0, 0.0)], stroke);
        painter.line_segment([center + vec2(3.0, 0.0), center + vec2(8.0, 0.0)], stroke);
        painter.line_segment([center - vec2(0.0, 8.0), center - vec2(0.0, 3.0)], stroke);
        painter.line_segment([center + vec2(0.0, 3.0), center + vec2(0.0, 8.0)], stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec2;
    use hp_app::view::{Camera2d, Camera3d, ViewKind};

    fn view(kind: ViewKind) -> ViewInfo {
        ViewInfo {
            index: 0,
            kind,
            rect: Rect::from_min_size(Pos2::ZERO, vec2(400.0, 300.0)),
            cam3d: Camera3d { yaw_deg: 0.0, pitch_deg: 0.0, ..Camera3d::default() },
            cam2d: Camera2d { center: DVec2::ZERO, zoom: 1.0 },
            pixels_per_point: 1.0,
            grid: 16.0,
        }
    }

    #[test]
    fn gizmo_directions() {
        let v = view(ViewKind::Top);
        assert_eq!(axis_screen_dir(&v, DVec3::X), vec2(1.0, 0.0));
        assert_eq!(axis_screen_dir(&v, DVec3::Y), vec2(0.0, -1.0));
        assert_eq!(axis_screen_dir(&v, DVec3::Z), vec2(0.0, 0.0));
        let v = view(ViewKind::Front);
        assert_eq!(axis_screen_dir(&v, DVec3::Y), vec2(1.0, 0.0));
        assert_eq!(axis_screen_dir(&v, DVec3::Z), vec2(0.0, -1.0));
        let v = view(ViewKind::Perspective);
        let y = axis_screen_dir(&v, DVec3::Y);
        assert!(y.x < -0.99);
        let z = axis_screen_dir(&v, DVec3::Z);
        assert!(z.y < -0.99);
    }
}
