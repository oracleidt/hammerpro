use egui::{Response, RichText, Ui};
use glam::DVec3;

use crate::t;

pub const HOVER_SECONDS: f32 = 0.12;

pub fn clip_x(rect: egui::Rect, outer: egui::Rect) -> egui::Rect {
    egui::Rect::from_x_y_ranges(rect.x_range(), outer.y_range()).intersect(outer)
}

pub fn crisp_width(ppp: f32, width: f32) -> f32 {
    let ppp = if ppp.is_finite() && ppp > 0.0 { ppp } else { 1.0 };
    (width * ppp).round().max(1.0) / ppp
}

pub fn crisp_stroke(painter: &egui::Painter, width: f32, color: egui::Color32) -> egui::Stroke {
    egui::Stroke::new(crisp_width(painter.pixels_per_point(), width), color)
}

pub fn crisp_center(painter: &egui::Painter, p: egui::Pos2) -> egui::Pos2 {
    egui::pos2(painter.round_to_pixel_center(p.x), painter.round_to_pixel_center(p.y))
}

pub fn crisp_rect(painter: &egui::Painter, r: egui::Rect) -> egui::Rect {
    use egui::emath::GuiRounding as _;
    r.round_to_pixels(painter.pixels_per_point())
}

pub fn hover_amounts(ui: &Ui, r: &Response) -> (f32, f32) {
    let live = r.enabled();
    let ctx = ui.ctx();
    let ease = egui::emath::easing::cubic_out;
    let hover =
        ctx.animate_bool_with_time_and_easing(r.id.with("hp_fx_hover"), live && r.hovered(), HOVER_SECONDS, ease);
    let press = ctx.animate_bool_with_time_and_easing(
        r.id.with("hp_fx_press"),
        live && r.is_pointer_button_down_on(),
        HOVER_SECONDS,
        ease,
    );
    (hover, press)
}

pub fn icon_toggle(ui: &mut Ui, icon: &str, selected: bool, tooltip: &str) -> Response {
    let mut button =
        egui::Button::selectable(selected, RichText::new(icon).size(16.0)).min_size(egui::vec2(28.0, 26.0));
    if selected {
        button = button.stroke(egui::Stroke::new(1.0, crate::theme::accent()));
    }
    let r = ui.add(button);
    r.on_hover_text(tooltip)
}

pub fn accent_button(ui: &mut Ui, text: &str) -> Response {
    use egui::emath::GuiRounding as _;
    let ppp = ui.ctx().pixels_per_point();
    let galley = egui::WidgetText::from(RichText::new(text).strong().color(crate::theme::on_accent())).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Button,
    );
    let pad = ui.spacing().button_padding;
    let size = egui::vec2(galley.size().x + 2.0 * pad.x + 6.0, (galley.size().y + 2.0 * pad.y).max(24.0));
    let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
    r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), text));
    if ui.is_rect_visible(rect) {
        let (hover, press) = hover_amounts(ui, &r);
        let accent = crate::theme::accent();
        let fill = accent.lerp_to_gamma(crate::theme::on_accent(), (hover * 0.14 + press * 0.12).min(0.3));
        let grown = rect.expand(hover * 1.0 - press * 0.5);
        let painter = ui.painter();
        painter.rect_filled(grown, crate::theme::RADIUS, fill);
        painter.rect_stroke(grown, crate::theme::RADIUS, egui::Stroke::new(1.0, accent), egui::StrokeKind::Inside);
        let origin = (grown.center() - galley.size() * 0.5).round_to_pixels(ppp);
        painter.galley(origin, galley, crate::theme::on_accent());
        if r.has_focus() {
            painter.rect_stroke(
                grown.expand(1.0),
                f32::from(crate::theme::RADIUS) + 1.0,
                ui.visuals().selection.stroke,
                egui::StrokeKind::Outside,
            );
        }
    }
    r
}

pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    accent_button(ui, text)
}

pub fn toggle(ui: &mut Ui, on: &mut bool, label: Option<&str>) -> Response {
    let track = egui::vec2(26.0, 14.0);
    let galley = label.map(|l| {
        egui::WidgetText::from(l).into_galley(
            ui,
            Some(egui::TextWrapMode::Extend),
            f32::INFINITY,
            egui::TextStyle::Button,
        )
    });
    let gap = if galley.is_some() { ui.spacing().icon_spacing + 2.0 } else { 0.0 };
    let text_size = galley.as_ref().map_or(egui::Vec2::ZERO, |g| g.size());
    let size = egui::vec2(track.x + gap + text_size.x, track.y.max(text_size.y).max(ui.spacing().interact_size.y));
    let (rect, mut response) = ui.allocate_exact_size(size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Switch, ui.is_enabled(), *on, label.unwrap_or_default())
    });
    if ui.is_rect_visible(rect) {
        let p = crate::theme::palette();
        let how_on = ui.ctx().animate_bool_responsive(response.id, *on);
        let track_rect = egui::Rect::from_min_size(egui::pos2(rect.left(), rect.center().y - track.y * 0.5), track);
        let hovered = response.hovered();
        let (fill, stroke) = if *on {
            (p.strong, p.strong)
        } else {
            (if hovered { p.bg4 } else { p.bg3 }, if hovered { p.border_strong } else { p.border })
        };
        let fill = if ui.is_enabled() { fill } else { fill.gamma_multiply(0.5) };
        let painter = ui.painter();
        painter.rect(track_rect, 7, fill, egui::Stroke::new(1.0, stroke), egui::StrokeKind::Inside);
        let r = track.y * 0.5 - 2.5;
        let x = egui::lerp((track_rect.left() + r + 2.5)..=(track_rect.right() - r - 2.5), how_on);
        let knob = if *on { p.inverse } else { p.text_weak };
        painter.circle_filled(egui::pos2(x, track_rect.center().y), r, knob);
        if let Some(galley) = galley {
            let color = if *on || hovered { p.strong } else { p.text };
            let color = if ui.is_enabled() { color } else { p.text_faint };
            let pos = egui::pos2(track_rect.right() + gap, rect.center().y - galley.size().y * 0.5);
            painter.galley(pos, galley, color);
        }
    }
    response
}

pub fn pill(ui: &mut Ui, text: &str, inverted: bool) -> Response {
    let (fg, bg, stroke) = if inverted {
        (crate::theme::on_accent(), crate::theme::accent(), crate::theme::accent())
    } else {
        (crate::theme::text_weak(), egui::Color32::TRANSPARENT, crate::theme::palette().border_strong)
    };
    let label = egui::Button::new(RichText::new(text).small().strong().color(fg))
        .fill(bg)
        .stroke(egui::Stroke::new(1.0, stroke))
        .corner_radius(9.0)
        .sense(egui::Sense::hover());
    ui.add(label)
}

pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(4.0);
    ui.label(RichText::new(title).strong());
    ui.separator();
}

fn palettes_id() -> egui::Id {
    egui::Id::unique("hp_palette_windows")
}

pub fn register_palette_window(ctx: &egui::Context, id: egui::Id) {
    ctx.data_mut(|d| {
        let list: &mut Vec<egui::Id> = d.get_temp_mut_or_default(palettes_id());
        if !list.contains(&id) {
            list.push(id);
        }
    });
}

pub fn palette_windows(ctx: &egui::Context) -> Vec<egui::Id> {
    ctx.data(|d| d.get_temp::<Vec<egui::Id>>(palettes_id()).unwrap_or_default())
}

pub fn progress_bar(fraction: f32) -> egui::ProgressBar {
    let fraction = if fraction.is_finite() { fraction.clamp(0.0, 1.0) } else { 0.0 };
    egui::ProgressBar::new(fraction).corner_radius(2)
}

pub fn search_box(ui: &mut Ui, text: &mut String, hint: &str) -> bool {
    search_field(ui, None, text, hint, false)
}

pub fn search_field(ui: &mut Ui, id: Option<egui::Id>, text: &mut String, hint: &str, focus: bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let side = ui.spacing().interact_size.y.max(18.0);
        let width = (ui.available_width() - side - ui.spacing().item_spacing.x).max(60.0);
        let mut edit = egui::TextEdit::singleline(text).hint_text(hint).desired_width(width);
        if let Some(id) = id {
            edit = edit.id(id);
        }
        let r = ui.add(edit);
        if focus {
            r.request_focus();
        }
        changed |= r.changed();
        if clear_button(ui, side).on_hover_text(t!("common.clear")).clicked() && !text.is_empty() {
            text.clear();
            changed = true;
        }
    });
    changed
}

fn clear_button(ui: &mut Ui, side: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click());
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, t!("common.clear")));
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&resp);
        ui.painter().rect(
            rect,
            visuals.corner_radius,
            visuals.weak_bg_fill,
            visuals.bg_stroke,
            egui::StrokeKind::Inside,
        );
        let galley = egui::WidgetText::from("✖").into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            side,
            egui::TextStyle::Small,
        );
        ui.painter().galley(rect.center() - galley.size() * 0.5, galley, visuals.text_color());
    }
    resp
}

pub fn vec3_edit(ui: &mut Ui, v: &mut DVec3, speed: f64) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        for (i, label) in ["X", "Y", "Z"].iter().enumerate() {
            ui.label(RichText::new(*label).color(crate::theme::text_weak()).strong());
            changed |= ui.add(egui::DragValue::new(&mut v[i]).speed(speed).max_decimals(3)).changed();
        }
    });
    changed
}

pub fn hammer_polygons() -> Vec<Vec<egui::Pos2>> {
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| vec![[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    let parts: [Vec<[f32; 2]>; 4] = [
        rect(0.14, 0.12, 0.30, 0.40),
        rect(0.29, 0.165, 0.72, 0.355),
        vec![[0.71, 0.165], [0.87, 0.205], [0.87, 0.315], [0.71, 0.355]],
        vec![[0.455, 0.35], [0.545, 0.35], [0.56, 0.90], [0.44, 0.90]],
    ];
    let (s, c) = (std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2);
    let turned: Vec<Vec<[f32; 2]>> = parts
        .iter()
        .map(|poly| {
            poly.iter()
                .map(|&[x, y]| {
                    let (dx, dy) = (x - 0.5, y - 0.5);
                    [dx * c - dy * s, dx * s + dy * c]
                })
                .collect()
        })
        .collect();
    let (mut min, mut max) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in turned.iter().flatten() {
        for i in 0..2 {
            min[i] = min[i].min(p[i]);
            max[i] = max[i].max(p[i]);
        }
    }
    let extent = (max[0] - min[0]).max(max[1] - min[1]);
    let scale = 0.74 / extent;
    let mid = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
    turned
        .into_iter()
        .map(|poly| {
            poly.into_iter().map(|[x, y]| egui::pos2(0.5 + (x - mid[0]) * scale, 0.5 + (y - mid[1]) * scale)).collect()
        })
        .collect()
}

pub fn paint_hammer(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    for poly in hammer_polygons() {
        let points = poly.into_iter().map(|p| rect.min + p.to_vec2() * rect.size()).collect();
        painter.add(egui::Shape::convex_polygon(points, color, egui::Stroke::NONE));
    }
}

pub fn logo(ui: &mut Ui, size: f32) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let p = crate::theme::palette();
        let painter = ui.painter();
        let radius = (size * 0.22).round().clamp(0.0, 255.0) as u8;
        painter.rect(rect, radius, p.inverse, egui::Stroke::new(1.0, p.border_strong), egui::StrokeKind::Inside);
        paint_hammer(painter, rect, p.strong);
    }
    response
}

pub const CHOICE_GAP: f32 = 14.0;

pub fn choice_row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = CHOICE_GAP;
        add(ui)
    })
    .inner
}

pub fn choice_row_wrapped<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = CHOICE_GAP;
        add(ui)
    })
    .inner
}

pub fn grid_row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.label(label);
    let r = add(ui);
    ui.end_row();
    r
}

pub const PLAIN_COORD_LIMIT: f64 = 1.0e9;

pub fn fmt_coord(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_owned()
    } else if v.is_infinite() {
        if v > 0.0 { "inf".to_owned() } else { "-inf".to_owned() }
    } else if v.abs() > PLAIN_COORD_LIMIT {
        format!("{v:.2e}")
    } else if (v - v.round()).abs() < 1e-6 {
        format!("{}", v.round() as i64)
    } else {
        format!("{v:.2}")
    }
}

pub fn fmt_vec3(v: DVec3) -> String {
    format!("{} {} {}", fmt_coord(v.x), fmt_coord(v.y), fmt_coord(v.z))
}

pub fn action_button(ui: &mut Ui, label: &str, shortcut: Option<&egui::KeyboardShortcut>, enabled: bool) -> bool {
    let mut b = egui::Button::new(label);
    if let Some(sc) = shortcut {
        b = b.shortcut_text(crate::actions::shortcut_label(sc));
    }
    ui.add_enabled(enabled, b).clicked()
}

pub fn model_lod_ui(ui: &mut Ui, lod: &mut crate::settings::ModelLodSettings) -> bool {
    use crate::settings::ModelLodSettings as L;
    let mut changed =
        ui.checkbox(&mut lod.enabled, t!("lab.lod.enabled")).on_hover_text(t!("lab.lod.enabled_tip")).changed();
    ui.add_enabled_ui(lod.enabled, |ui| {
        ui.horizontal(|ui| {
            ui.label(t!("lab.lod.levels"));
            changed |= ui
                .add(egui::DragValue::new(&mut lod.levels).range(L::MIN_LEVELS..=L::MAX_LEVELS))
                .on_hover_text(t!("lab.lod.levels_tip"))
                .changed();
            ui.label(t!("lab.lod.reduction"));
            let mut percent = (lod.reduction * 100.0).round();
            if ui
                .add(egui::DragValue::new(&mut percent).range(20.0..=90.0).suffix(" %"))
                .on_hover_text(t!("lab.lod.reduction_tip"))
                .changed()
            {
                lod.reduction = percent / 100.0;
                changed = true;
            }
        });
        ui.label(t!("lab.lod.distances_tip"));
        for (index, distance) in lod.distances.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("LOD {}", index + 1));
                let mut automatic = distance.is_none();
                if ui.checkbox(&mut automatic, t!("lab.lod.automatic")).changed() {
                    *distance = (!automatic).then_some(64.0 * f64::from(1u32 << index));
                    changed = true;
                }
                if let Some(value) = distance {
                    changed |= ui
                        .add(egui::DragValue::new(value).speed(1.0).range(0.01..=1_000_000.0))
                        .on_hover_text(t!("lab.lod.metric_tip"))
                        .changed();
                }
            });
        }
    });
    if changed {
        *lod = lod.clamped();
    }
    changed
}

pub fn vsep(ui: &mut Ui) {
    let height = ui.spacing().interact_size.y;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.spacing().item_spacing.x + 2.0, height), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let x = painter.round_to_pixel_center(rect.center().x);
        let stroke = crisp_stroke(painter, 1.0, ui.visuals().widgets.noninteractive.bg_stroke.color);
        painter.vline(x, rect.y_range().shrink(2.0), stroke);
    }
}

pub const SPINNER_STEP: f64 = 1.0 / 12.0;

pub fn paced_spinner(ui: &mut Ui, size: f32) -> Response {
    spinner_with(ui, size, crate::runtime::render::animations_running())
}

fn spinner_with(ui: &mut Ui, size: f32, running: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let now = ui.input(|i| i.time);
    let time = (now / SPINNER_STEP).floor() * SPINNER_STEP;
    if running {
        let wait = (time + SPINNER_STEP - now).max(0.001);
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(wait));
    }
    let color = ui.visuals().strong_text_color();
    let radius = rect.height().min(rect.width()) * 0.5 - 2.0;
    let count = (radius.round() as u32).clamp(8, 128);
    let start = time * std::f64::consts::TAU;
    let end = start + 240f64.to_radians() * time.sin();
    let points: Vec<egui::Pos2> = (0..count)
        .map(|i| {
            let angle = egui::emath::lerp(start..=end, f64::from(i) / f64::from(count));
            let (sin, cos) = angle.sin_cos();
            rect.center() + radius * egui::vec2(cos as f32, sin as f32)
        })
        .collect();
    ui.painter().add(egui::Shape::line(points, egui::Stroke::new(3.0, color)));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spinner_repaint_delay(running: bool, time: f64) -> std::time::Duration {
        let ctx = egui::Context::default();
        let mut delay = std::time::Duration::MAX;
        for at in [time - 2.0, time - 1.0, time] {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
                time: Some(at),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    spinner_with(ui, 16.0, running);
                });
            });
            delay = out.viewport_output.values().map(|v| v.repaint_delay).min().unwrap_or(std::time::Duration::MAX);
        }
        delay
    }

    #[test]
    fn the_spinner_turns_a_dozen_times_a_second_and_stops_with_the_animations() {
        let delay = spinner_repaint_delay(true, 10.0);
        assert!(delay > std::time::Duration::from_millis(20), "not every frame: {delay:?}");
        assert!(delay <= std::time::Duration::from_secs_f64(SPINNER_STEP), "{delay:?}");
        let mid = spinner_repaint_delay(true, 10.0 + SPINNER_STEP * 0.5);
        assert!(mid < delay, "the next step is on the clock: {mid:?} {delay:?}");
        assert_eq!(spinner_repaint_delay(false, 10.0), std::time::Duration::MAX, "paused like the other animations");
    }

    #[test]
    fn coordinates_stay_readable() {
        assert_eq!(fmt_coord(16.0), "16");
        assert_eq!(fmt_coord(-0.5), "-0.50");
        assert_eq!(fmt_coord(32.4999999999), "32.50");
        assert_eq!(fmt_coord(1.0e9), "1000000000");
        assert_eq!(fmt_coord(2.0e9), "2.00e9");
        assert_eq!(fmt_coord(-1.0e300), "-1.00e300");
        assert_eq!(fmt_coord(f64::NAN), "NaN");
        assert_eq!(fmt_coord(f64::INFINITY), "inf");
        assert_eq!(fmt_coord(f64::NEG_INFINITY), "-inf");
        for v in [1e19, i64::MAX as f64, f64::MAX, -f64::MAX] {
            assert!(!fmt_coord(v).contains("922337203685477"), "{v}: {}", fmt_coord(v));
        }
        assert_eq!(fmt_vec3(DVec3::new(16.0, -0.5, 2.0e9)), "16 -0.50 2.00e9");
        assert_eq!(fmt_vec3(DVec3::new(f64::NAN, f64::INFINITY, 1e300)), "NaN inf 1.00e300");
    }

    #[test]
    fn the_hammer_mark_fits_its_square() {
        let polys = hammer_polygons();
        assert_eq!(polys.len(), 4);
        let (mut min, mut max) = (egui::pos2(f32::MAX, f32::MAX), egui::pos2(f32::MIN, f32::MIN));
        for p in polys.iter().flatten() {
            assert!(p.x.is_finite() && p.y.is_finite());
            min = min.min(*p);
            max = max.max(*p);
        }
        assert!(min.x >= 0.1 && min.y >= 0.1 && max.x <= 0.9 && max.y <= 0.9, "{min:?} {max:?}");
        let extent = (max.x - min.x).max(max.y - min.y);
        assert!((extent - 0.74).abs() < 1e-4, "{extent}");
        let head = polys[0].iter().fold(egui::Vec2::ZERO, |a, p| a + p.to_vec2()) / 4.0;
        let grip = polys[3].iter().fold(egui::Vec2::ZERO, |a, p| a + p.to_vec2()) / 4.0;
        assert!(head.y < 0.5 && grip.x < 0.5 && grip.y > 0.5, "{head:?} {grip:?}");
    }

    #[test]
    fn a_click_flips_the_toggle() {
        let mut state = crate::testing::test_state();
        let mut on = true;
        let mut rect = egui::Rect::NOTHING;
        crate::testing::run_ui(&mut state, |ui, _| rect = toggle(ui, &mut on, Some("Launch")).rect);
        assert!(on && rect.width() > 26.0, "drawing changes nothing");
        let ctx = egui::Context::default();
        let mut changed = 0;
        for (i, events) in [
            vec![],
            vec![egui::Event::PointerMoved(rect.center())],
            vec![egui::Event::PointerButton {
                pos: rect.center(),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
            vec![egui::Event::PointerButton {
                pos: rect.center(),
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        ]
        .into_iter()
        .enumerate()
        {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
                events,
                time: Some(i as f64 * 0.1),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    if toggle(ui, &mut on, Some("Launch")).changed() {
                        changed += 1;
                    }
                });
            });
            out.textures_delta.clear();
        }
        assert!(!on, "one click turns it off");
        assert_eq!(changed, 1);
    }

    #[test]
    fn widgets_draw_on_both_themes() {
        let mut state = crate::testing::test_state();
        crate::testing::run_ui(&mut state, |ui, _| {
            logo(ui, 64.0);
            logo(ui, 0.0);
            pill(ui, "ON", true);
            pill(ui, "OFF", false);
            let mut on = true;
            toggle(ui, &mut on, Some("Launch the game"));
            toggle(ui, &mut on, None);
            primary_button(ui, "Compile");
            icon_toggle(ui, "A", true, "tip");
            let mut v = DVec3::new(1.0, 2.0, 3.0);
            vec3_edit(ui, &mut v, 1.0);
        });
    }
}
