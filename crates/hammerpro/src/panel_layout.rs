pub const CONSOLE_MIN: f32 = 60.0;
pub const DOCK_MIN: f32 = 220.0;
pub const CENTER_MIN_WIDTH: f32 = 360.0;
pub const CENTER_MIN_HEIGHT: f32 = 240.0;

pub fn console_max(available_height: f32) -> f32 {
    (available_height - CENTER_MIN_HEIGHT).max(CONSOLE_MIN).min((available_height * 0.5).max(CONSOLE_MIN))
}

pub fn dock_max(available_width: f32) -> f32 {
    (available_width - CENTER_MIN_WIDTH).max(DOCK_MIN).min((available_width * 0.6).max(DOCK_MIN))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Right,
    Bottom,
}

impl Side {
    fn of(self, v: egui::Vec2) -> f32 {
        match self {
            Side::Right => v.x,
            Side::Bottom => v.y,
        }
    }

    fn panel(self, id_salt: &'static str) -> egui::Panel {
        match self {
            Side::Right => egui::Panel::right(id_salt),
            Side::Bottom => egui::Panel::bottom(id_salt),
        }
    }
}

pub struct SizedPanel {
    pub size: f32,
    pub committed: Option<f32>,
}

pub fn sized_panel(
    ui: &mut egui::Ui,
    side: Side,
    id_salt: &'static str,
    wanted: f32,
    range: (f32, f32),
    add: impl FnOnce(&mut egui::Ui),
) -> SizedPanel {
    let id = ui.scope_id().with(id_salt);
    let resize_id = id.with("__resize");
    let (min, max) = (range.0, range.1.max(range.0));
    let target = if wanted.is_finite() { wanted.clamp(min, max) } else { min };
    let user_drag = ui.ctx().read_response(resize_id).is_some_and(|r| r.dragged());
    let stored = egui::PanelState::load(ui.ctx(), id).map(|s| side.of(s.size()));
    let panel = side.panel(id_salt).resizable(true).default_size(target).min_size(min).max_size(max);
    let panel = match stored {
        Some(size) if !user_drag && (size - target).abs() > 0.5 => panel.exact_size(target),
        _ => panel,
    };
    let r = panel.show(ui, |ui| bounded(ui, add));
    let size = side.of(r.response.rect.size());
    let committed = ui.ctx().read_response(resize_id).filter(|r| r.drag_stopped()).map(|_| size.round());
    SizedPanel { size, committed }
}

pub fn fitted_panel_height(content: f32, overhead: f32) -> f32 {
    let content = if content.is_finite() { content.max(0.0) } else { 0.0 };
    (content + overhead.max(0.0)).ceil()
}

pub fn fitted_top<R>(
    ui: &mut egui::Ui,
    id_salt: &'static str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let memory = ui.scope_id().with(id_salt).with("__content_height");
    let frame = egui::Frame::side_top_panel(ui.style());
    let line = ui.style().visuals.widgets.noninteractive.bg_stroke.width.round();
    let overhead = frame.total_margin().sum().y + line;
    let known: Option<f32> = ui.data(|d| d.get_temp(memory));
    let mut panel = egui::Panel::top(id_salt).frame(frame).resizable(false);
    if let Some(content) = known {
        panel = panel.exact_size(fitted_panel_height(content, overhead));
    }
    panel.show(ui, |ui| {
        let start = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 0.0));
        let builder = egui::UiBuilder::new().max_rect(start).layout(egui::Layout::top_down(egui::Align::Min));
        let r = ui.scope_builder(builder, add);
        let content = r.response.rect.height();
        if known.is_none_or(|k| (k - content).abs() > 0.5) {
            ui.data_mut(|d| d.insert_temp(memory, content));
            ui.ctx().request_repaint();
        }
        r.inner
    })
}

pub fn bounded<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let rect = ui.available_rect_before_wrap();
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(*ui.layout()));
    child.shrink_clip_rect(rect);
    let inner = add(&mut child);
    ui.advance_cursor_after_rect(rect);
    inner
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZES: [(f32, f32); 4] = [(1280.0, 720.0), (1366.0, 768.0), (1920.0, 1080.0), (2560.0, 1440.0)];

    fn frame(size: (f32, f32), console_h: f32, dock_w: f32) -> [egui::Rect; 5] {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.0, size.1))),
            ..Default::default()
        };
        let mut rects = [egui::Rect::NOTHING; 5];
        let _ = ctx.run_ui(input, |ui| {
            rects[0] = egui::Panel::top("t").exact_size(90.0).show(ui, |_| {}).response.rect;
            rects[1] = egui::Panel::bottom("s").exact_size(24.0).show(ui, |_| {}).response.rect;
            rects[2] = egui::Panel::left("l").exact_size(48.0).show(ui, |_| {}).response.rect;
            let w = dock_max(ui.available_width());
            rects[3] = egui::Panel::right("d")
                .resizable(true)
                .default_size(dock_w)
                .min_size(DOCK_MIN)
                .max_size(w)
                .show(ui, |_| {})
                .response
                .rect;
            let h = console_max(ui.available_height());
            let c = egui::Panel::bottom("c")
                .resizable(true)
                .default_size(console_h)
                .min_size(CONSOLE_MIN)
                .max_size(h)
                .show(ui, |_| {})
                .response
                .rect;
            let center = egui::CentralPanel::default().show(ui, |_| {}).response.rect;
            rects[4] = c;
            assert!(center.width() >= CENTER_MIN_WIDTH - 1.0, "{size:?} center {center:?}");
            assert!(center.height() >= CENTER_MIN_HEIGHT - 1.0, "{size:?} center {center:?}");
            assert!(!c.intersects(center.shrink(1.0)) && !rects[3].intersects(center.shrink(1.0)));
        });
        rects
    }

    #[test]
    fn panels_never_overlap() {
        for size in SIZES {
            for (console_h, dock_w) in [(150.0, 380.0), (5000.0, 5000.0), (1.0, 1.0)] {
                let r = frame(size, console_h, dock_w);
                for i in 0..r.len() {
                    for j in (i + 1)..r.len() {
                        assert!(
                            !r[i].shrink(0.5).intersects(r[j].shrink(0.5)),
                            "{size:?} {i} {j} {:?} {:?}",
                            r[i],
                            r[j]
                        );
                    }
                }
                let dock = r[3];
                assert!(r[4].max.x <= dock.min.x + 0.5, "console must stay left of the dock");
                assert!(dock.height() >= size.1 - 90.0 - 24.0 - 2.0, "dock keeps its full height");
            }
        }
    }

    fn dock_frame(ctx: &egui::Context, size: (f32, f32), wanted: f32, events: Vec<egui::Event>) -> SizedPanel {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.0, size.1))),
            events,
            ..Default::default()
        };
        let mut out = None;
        let _ = ctx.run_ui(input, |ui| {
            let max = dock_max(ui.available_width());
            out = Some(sized_panel(ui, Side::Right, "hp_dock", wanted, (DOCK_MIN, max), |ui| {
                ui.horizontal(|ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    ui.label("a very long row of buttons that never wraps and asks for far more width than the dock");
                });
            }));
            egui::CentralPanel::default().show(ui, |_| {});
        });
        out.expect("frame")
    }

    #[test]
    fn wide_content_never_pushes_the_dock_back() {
        let ctx = egui::Context::default();
        for _ in 0..6 {
            let r = dock_frame(&ctx, (1600.0, 900.0), DOCK_MIN, Vec::new());
            assert!((r.size - DOCK_MIN).abs() < 0.5, "{}", r.size);
        }
    }

    #[test]
    fn the_saved_size_is_applied_and_restored_after_a_small_window() {
        let ctx = egui::Context::default();
        assert!((dock_frame(&ctx, (1600.0, 900.0), 500.0, Vec::new()).size - 500.0).abs() < 0.5);
        let small = dock_frame(&ctx, (800.0, 600.0), 500.0, Vec::new()).size;
        assert!((small - dock_max(800.0)).abs() < 1.0, "{small}");
        let _ = dock_frame(&ctx, (1600.0, 900.0), 500.0, Vec::new());
        assert!((dock_frame(&ctx, (1600.0, 900.0), 500.0, Vec::new()).size - 500.0).abs() < 0.5);
        let _ = dock_frame(&ctx, (1600.0, 900.0), 300.0, Vec::new());
        assert!((dock_frame(&ctx, (1600.0, 900.0), 300.0, Vec::new()).size - 300.0).abs() < 0.5);
    }

    #[test]
    fn dragging_the_splitter_shrinks_widens_and_commits() {
        let ctx = egui::Context::default();
        let mut wanted = 380.0;
        for _ in 0..3 {
            let _ = dock_frame(&ctx, (1600.0, 900.0), wanted, Vec::new());
        }
        for target in [DOCK_MIN, 640.0, 200.0] {
            let edge = egui::pos2(1600.0 - wanted, 450.0);
            let goal = egui::pos2(1600.0 - target, 450.0);
            let button = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            let steps = [
                vec![egui::Event::PointerMoved(edge)],
                vec![button(edge, true)],
                vec![egui::Event::PointerMoved(edge.lerp(goal, 0.5))],
                vec![egui::Event::PointerMoved(goal)],
                vec![egui::Event::PointerMoved(goal)],
                vec![button(goal, false)],
            ];
            let mut committed = None;
            for events in steps {
                let r = dock_frame(&ctx, (1600.0, 900.0), wanted, events);
                committed = committed.or(r.committed);
            }
            let got = committed.expect("the drag commits a size");
            assert!((got - target.max(DOCK_MIN)).abs() <= 2.0, "{target}: {got}");
            wanted = got;
            for _ in 0..3 {
                let r = dock_frame(&ctx, (1600.0, 900.0), wanted, Vec::new());
                assert!((r.size - wanted).abs() < 0.5, "{target}: kept {} wanted {wanted}", r.size);
            }
        }
    }

    #[test]
    fn limits_are_sane() {
        assert!(console_max(720.0 - 114.0) >= CONSOLE_MIN);
        assert!(console_max(100.0) >= CONSOLE_MIN);
        assert!(dock_max(1000.0) >= DOCK_MIN);
        assert!(console_max(1000.0) <= 500.0);
    }

    fn top_height(ctx: &egui::Context, rows: usize) -> (f32, f32) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1920.0, 1080.0))),
            ..Default::default()
        };
        let mut out = (0.0, 0.0);
        let _ = ctx.run_ui(input, |ui| {
            let r = fitted_top(ui, "hp_top_test", |ui| {
                ui.horizontal_top(|ui| ui.separator());
                for _ in 0..rows {
                    ui.allocate_exact_size(egui::vec2(200.0, 24.0), egui::Sense::hover());
                }
                ui.min_rect().height()
            });
            out = (r.response.rect.height(), r.inner);
        });
        out
    }

    fn settled(ctx: &egui::Context, rows: usize) -> (f32, f32) {
        let mut last = top_height(ctx, rows);
        for _ in 0..3 {
            last = top_height(ctx, rows);
        }
        last
    }

    #[test]
    fn top_bar_fits_its_rows_and_shrinks_back() {
        let ctx = egui::Context::default();
        let (one, content_one) = settled(&ctx, 1);
        let (two, content_two) = settled(&ctx, 2);
        let (again, _) = settled(&ctx, 1);
        let overhead = one - content_one;
        assert!((0.0..20.0).contains(&overhead), "overhead {overhead}");
        assert!((two - content_two - overhead).abs() <= 1.0, "two rows {two} content {content_two}");
        assert!(two - one >= 24.0, "a second row adds its height: {one} -> {two}");
        assert!((again - one).abs() <= 0.5, "shrinks back to one row: {one} vs {again}");
        assert_eq!(fitted_panel_height(24.0, 9.0), 33.0);
        assert_eq!(fitted_panel_height(f32::NAN, 9.0), 9.0);
    }
}
