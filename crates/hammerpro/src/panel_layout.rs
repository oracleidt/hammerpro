pub const CONSOLE_MIN: f32 = 60.0;
pub const CONSOLE_DEFAULT: f32 = 150.0;
pub const DOCK_MIN: f32 = 260.0;
pub const DOCK_DEFAULT: f32 = 380.0;
pub const CENTER_MIN_WIDTH: f32 = 360.0;
pub const CENTER_MIN_HEIGHT: f32 = 240.0;

pub fn console_max(available_height: f32) -> f32 {
    (available_height - CENTER_MIN_HEIGHT).max(CONSOLE_MIN).min((available_height * 0.5).max(CONSOLE_MIN))
}

pub fn dock_max(available_width: f32) -> f32 {
    (available_width - CENTER_MIN_WIDTH).max(DOCK_MIN).min((available_width * 0.6).max(DOCK_MIN))
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
            for (console_h, dock_w) in [(CONSOLE_DEFAULT, DOCK_DEFAULT), (5000.0, 5000.0), (1.0, 1.0)] {
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

    #[test]
    fn limits_are_sane() {
        assert!(console_max(720.0 - 114.0) >= CONSOLE_MIN);
        assert!(console_max(100.0) >= CONSOLE_MIN);
        assert!(dock_max(1000.0) >= DOCK_MIN);
        assert!(console_max(1000.0) <= 500.0);
    }
}
