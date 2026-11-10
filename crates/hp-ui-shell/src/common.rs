use egui::{Color32, RichText, Ui};
use hp_app::{Action, AppState, Severity, t};

pub const GRID_SIZES: [f64; 16] =
    [0.125, 0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0, 256.0, 512.0, 1024.0, 2048.0, 4096.0];

pub fn fmt_grid(g: f64) -> String {
    if (g - g.round()).abs() < 1e-9 {
        format!("{}", g.round() as i64)
    } else {
        let s = format!("{g:.3}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

pub use hp_app::actions::Check;

pub fn action_enabled(state: &AppState, action: Action) -> bool {
    action.enabled(state)
}

pub fn action_check(state: &AppState, action: Action) -> Check {
    action.check(state)
}

pub fn tool_icon(tool: hp_app::ToolId) -> &'static str {
    tool.icon()
}

pub fn action_icon(action: Action) -> Option<&'static str> {
    if let Some(tool) = action.tool() {
        return Some(tool_icon(tool));
    }
    action.icon()
}

pub fn tooltip_with_shortcut(label: &str, shortcut: Option<egui::KeyboardShortcut>) -> String {
    match shortcut {
        Some(sc) => format!("{label} ({})", hp_app::actions::shortcut_label(&sc)),
        None => label.to_owned(),
    }
}

pub fn action_tooltip(state: &AppState, action: Action) -> String {
    tooltip_with_shortcut(action.label(), state.keys.shortcut(action))
}

pub fn with_action_tooltip(response: egui::Response, state: &AppState, action: Action) -> egui::Response {
    response
        .on_hover_ui(|ui| {
            ui.label(action_tooltip(state, action));
        })
        .on_disabled_hover_ui(|ui| {
            ui.label(action_tooltip(state, action));
        })
}

pub fn category_label(cat: hp_app::actions::Category) -> &'static str {
    t!(cat.label_key())
}

pub const CATEGORIES: [hp_app::actions::Category; 8] = hp_app::actions::Category::ALL;

pub fn weak(ui: &Ui) -> Color32 {
    ui.visuals().weak_text_color()
}

pub fn severity_color(visuals: &egui::Visuals, severity: Severity) -> Color32 {
    match severity {
        Severity::Info => visuals.weak_text_color(),
        Severity::Success => visuals.text_color(),
        Severity::Warning | Severity::Error => visuals.strong_text_color(),
    }
}

pub fn accent_bg(visuals: &egui::Visuals) -> Color32 {
    visuals.selection.bg_fill
}

pub fn accent_fg(visuals: &egui::Visuals) -> Color32 {
    visuals.strong_text_color()
}

pub fn accent_text(visuals: &egui::Visuals) -> Color32 {
    visuals.strong_text_color()
}

pub fn close_on_escape(ui: &mut Ui) {
    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        ui.close();
    }
}

pub fn window_layer(id: egui::Id) -> egui::LayerId {
    egui::LayerId::new(egui::Order::Middle, id)
}

pub fn modal_layer(id: egui::Id) -> egui::LayerId {
    egui::LayerId::new(egui::Order::Foreground, id)
}

pub fn is_top_dialog(ctx: &egui::Context, layer: egui::LayerId) -> bool {
    let palettes = hp_app::widgets::palette_windows(ctx);
    ctx.memory(|m| {
        if let Some(modal) = m.top_modal_layer() {
            return modal == layer;
        }
        let areas = m.areas();
        let top = m
            .layer_ids()
            .filter(|l| {
                l.order == egui::Order::Middle
                    && areas.is_visible(l)
                    && areas.parent_layer(*l).is_none()
                    && !palettes.contains(&l.id)
            })
            .last();
        top == Some(layer)
    })
}

pub fn escape_pressed(ctx: &egui::Context) -> bool {
    ctx.input(|i| {
        i.events.iter().any(|e| {
            matches!(
                e,
                egui::Event::Key { key: egui::Key::Escape, pressed: true, modifiers, .. } if !modifiers.any()
            )
        })
    })
}

#[derive(Clone, Copy, Debug, Default)]
pub struct EscapeGuard {
    popup_before: bool,
}

impl EscapeGuard {
    pub fn should_close(&mut self, ctx: &egui::Context, layer: egui::LayerId) -> bool {
        let popup_before = std::mem::take(&mut self.popup_before);
        if !escape_pressed(ctx) || popup_before || egui::Popup::is_any_open(ctx) || !is_top_dialog(ctx, layer) {
            return false;
        }
        ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    }

    pub fn finish(&mut self, ctx: &egui::Context, layer: egui::LayerId) {
        if is_top_dialog(ctx, layer) {
            ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        }
        self.popup_before = egui::Popup::is_any_open(ctx);
    }

    pub fn reset(&mut self) {
        self.popup_before = false;
    }
}

pub fn search_field(ui: &mut Ui, id: egui::Id, text: &mut String, hint: &str, focus: bool) -> bool {
    hp_app::widgets::search_field(ui, Some(id), text, hint, focus)
}

pub fn status_toggle(ui: &mut Ui, text: &str, on: bool) -> egui::Response {
    let font = egui::TextStyle::Small.resolve(ui.style());
    let visuals = ui.visuals().clone();
    let color = if on { visuals.text_color() } else { visuals.weak_text_color() };
    let galley = ui.painter().layout_no_wrap(text.to_owned(), font, color);
    const DOT: f32 = 6.0;
    const PAD: f32 = 6.0;
    const GAP: f32 = 5.0;
    let height = (galley.size().y + 2.0).max(DOT + 4.0);
    let size = egui::vec2(PAD + DOT + GAP + galley.size().x + PAD, height);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, ui.is_enabled(), on, text));
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        let radius = hp_app::theme::RADIUS;
        if response.is_pointer_button_down_on() {
            painter.rect_filled(rect, radius, visuals.widgets.active.weak_bg_fill);
        } else if response.hovered() {
            painter.rect_filled(rect, radius, visuals.widgets.hovered.weak_bg_fill);
        }
        let center = egui::pos2(rect.left() + PAD + DOT / 2.0, rect.center().y);
        if on {
            painter.circle_filled(center, DOT / 2.0, hp_app::theme::accent());
        } else {
            painter.circle_stroke(center, DOT / 2.0 - 0.5, egui::Stroke::new(1.0, hp_app::theme::palette().text_faint));
        }
        let text_pos = egui::pos2(rect.left() + PAD + DOT + GAP, rect.center().y - galley.size().y / 2.0);
        painter.galley(text_pos, galley, color);
    }
    response
}

pub fn pill(ui: &mut Ui, text: &str, on: bool) -> egui::Response {
    let visuals = ui.visuals();
    let (fg, bg) =
        if on { (accent_fg(visuals), accent_bg(visuals)) } else { (visuals.weak_text_color(), Color32::TRANSPARENT) };
    let button = egui::Button::new(RichText::new(text).small().color(fg))
        .fill(bg)
        .stroke(egui::Stroke::new(
            1.0,
            if on { hp_app::theme::accent() } else { ui.visuals().widgets.noninteractive.bg_stroke.color },
        ))
        .corner_radius(8.0)
        .min_size(egui::vec2(0.0, 16.0));
    ui.add(button)
}

pub fn lazy_hover_text(r: egui::Response, text: impl FnOnce() -> String) -> egui::Response {
    if r.hovered() || r.ctx.has_accesskit_node(r.id) { r.on_hover_text(text()) } else { r }
}

pub fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(RichText::new(text).strong().color(ui.visuals().strong_text_color()));
    let rect = ui.available_rect_before_wrap();
    let y = rect.top();
    ui.painter().hline(
        rect.left()..=rect.right(),
        y,
        egui::Stroke::new(1.0, hp_app::theme::accent().gamma_multiply(0.5)),
    );
    ui.add_space(4.0);
}

pub fn fixed_label(ui: &mut Ui, width: f32, height: f32, text: impl Into<egui::WidgetText>) -> egui::Response {
    ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_size(egui::vec2(width, height));
        ui.add(egui::Label::new(text).truncate())
    })
    .inner
}

pub fn fmt_clock(seconds: f64) -> String {
    let total = if seconds.is_finite() && seconds > 0.0 { seconds as u64 } else { 0 };
    format!("{:02}:{:02}:{:02}", total / 3600, (total / 60) % 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_format() {
        assert_eq!(fmt_grid(16.0), "16");
        assert_eq!(fmt_grid(0.125), "0.125");
        assert_eq!(fmt_grid(0.5), "0.5");
        assert_eq!(fmt_grid(1024.0), "1024");
    }

    #[test]
    fn clock_format() {
        assert_eq!(fmt_clock(0.0), "00:00:00");
        assert_eq!(fmt_clock(3725.9), "01:02:05");
        assert_eq!(fmt_clock(f64::NAN), "00:00:00");
    }

    #[test]
    fn enabled_states_follow_document() {
        let mut state = hp_app::testing::test_state();
        assert!(!action_enabled(&state, Action::Copy));
        assert!(!action_enabled(&state, Action::Undo));
        assert!(action_enabled(&state, Action::SaveMap));
        assert!(action_enabled(&state, Action::Paste));
        assert!(!action_enabled(&state, Action::UnloadPointfile));
        state.doc.select_all();
        assert!(action_enabled(&state, Action::Copy));
        assert!(action_enabled(&state, Action::Center2dOnSelection));
    }

    #[test]
    fn check_states_follow_settings() {
        let mut state = hp_app::testing::test_state();
        state.settings.edit.snap_to_grid = false;
        assert_eq!(action_check(&state, Action::ToggleSnap), Check::Toggle(false));
        state.settings.view.render_mode = hp_app::settings::RenderModeSetting::Wireframe;
        assert_eq!(action_check(&state, Action::RenderWireframe), Check::Radio(true));
        assert_eq!(action_check(&state, Action::RenderTextured), Check::Radio(false));
        assert_eq!(action_check(&state, Action::ToolSelect), Check::Radio(true));
        assert_eq!(action_check(&state, Action::SaveMap), Check::None);
    }
}
