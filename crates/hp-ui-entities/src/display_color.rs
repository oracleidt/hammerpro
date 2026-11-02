use egui::{Color32, CornerRadius, Id, RichText, Sense, Stroke, Ui, vec2};
use hp_app::settings::Settings;
use hp_app::{AppState, Severity, t, tf};
use hp_editor::display_color::{DisplayColor, DisplayColorState, PALETTE};
use hp_editor::{Document, ObjRef};

use crate::edit::{Op, apply};

const SWATCH: f32 = 16.0;
const NEUTRAL: [u8; 3] = [128, 128, 128];

fn rgb32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn hex(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

fn swatch(ui: &mut Ui, c: [u8; 3], current: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(SWATCH, SWATCH), Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(3), rgb32(c));
    let stroke = if current {
        Stroke::new(2.0, ui.visuals().strong_text_color())
    } else if resp.hovered() {
        Stroke::new(1.0, ui.visuals().strong_text_color())
    } else {
        Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.6))
    };
    painter.rect_stroke(rect, CornerRadius::same(3), stroke, egui::StrokeKind::Inside);
    resp.on_hover_text(hex(c))
}

pub(crate) fn section(
    ui: &mut Ui,
    doc: &Document,
    settings: &mut Settings,
    settings_dirty: &mut bool,
    objs: &[ObjRef],
    ops: &mut Vec<Op>,
) {
    egui::CollapsingHeader::new(t!("entities.display_color"))
        .id_salt("hp_ui_entities.display_color")
        .default_open(true)
        .show(ui, |ui| body(ui, doc, settings, settings_dirty, objs, ops));
}

pub(crate) fn body(
    ui: &mut Ui,
    doc: &Document,
    settings: &mut Settings,
    settings_dirty: &mut bool,
    objs: &[ObjRef],
    ops: &mut Vec<Op>,
) {
    if objs.is_empty() {
        ui.label(RichText::new(t!("entities.display_color_none")).weak());
        return;
    }
    let state = doc.display_color_state(objs);
    let current = match state {
        DisplayColorState::Custom(c) => Some(c),
        _ => None,
    };
    let mut push = |change: DisplayColor, merge: bool| {
        ops.push(Op::DisplayColor { objs: objs.to_vec(), change, merge });
    };
    ui.horizontal(|ui| {
        let mut c = current.unwrap_or(NEUTRAL);
        let resp = ui.color_edit_button_srgb(&mut c).on_hover_text(t!("entities.display_color_tip"));
        if resp.changed() {
            push(DisplayColor::Set(c), true);
        }
        let note = match state {
            DisplayColorState::Custom(c) => hex(c),
            DisplayColorState::Mixed => t!("common.multiple_values").to_owned(),
            _ => t!("entities.display_color_default").to_owned(),
        };
        ui.label(RichText::new(note).weak());
    });
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
        for c in PALETTE {
            if swatch(ui, c, current == Some(c)).clicked() {
                push(DisplayColor::Set(c), false);
            }
        }
    });
    ui.horizontal_wrapped(|ui| {
        let random = ui.small_button(t!("entities.display_color_random"));
        if random.on_hover_text(t!("entities.display_color_random_tip")).clicked() {
            push(DisplayColor::Random, false);
        }
        let reset_button = egui::Button::new(t!("entities.display_color_reset")).small();
        let reset = ui.add_enabled(state != DisplayColorState::Default, reset_button);
        if reset.on_hover_text(t!("entities.display_color_reset_tip")).clicked() {
            push(DisplayColor::Reset, false);
        }
    });
    if !settings.render.object_colors_2d {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(t!("entities.display_color_off")).weak().small());
            if ui.small_button(t!("entities.display_color_turn_on")).clicked() {
                settings.render.object_colors_2d = true;
                *settings_dirty = true;
            }
        });
    }
}

#[derive(Default)]
pub(crate) struct DisplayColorWindow;

impl DisplayColorWindow {
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.entities.display_color_window {
            return;
        }
        let mut open = true;
        let mut ops = Vec::new();
        let objs = state.doc.selected_objects();
        let AppState { doc, settings, settings_dirty, .. } = state;
        let center = ctx.content_rect().center() - vec2(110.0, 90.0);
        let pos = ctx.input(|i| i.pointer.latest_pos()).map_or(center, |p| p + vec2(16.0, 16.0));
        let resp = hp_app::chrome::Window::new(t!("entities.display_color_title"))
            .id(Id::unique("hp_ui_entities.display_color_window"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(220.0)
            .default_pos(pos)
            .show(ctx, |ui| {
                if !objs.is_empty() {
                    ui.label(RichText::new(tf!("entities.display_color_count", objs.len())).weak());
                }
                body(ui, doc, settings, settings_dirty, &objs, &mut ops);
            });
        if crate::dialogs::escape_closes(ctx, resp.as_ref().map(|r| r.response.layer_id), false) {
            open = false;
        }
        for op in ops {
            if let Err(e) = apply(&mut state.doc, None, op) {
                state.notify(Severity::Error, tf!("entities.edit_failed", e));
            }
        }
        state.runtime.entities.display_color_window = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_and_window_draw_without_editing() {
        crate::register_translations();
        let mut s = hp_app::testing::test_state();
        s.doc.set_selection(&[ObjRef::Solid(2), ObjRef::Entity(6)]);
        let steps = s.doc.history_len().0;
        s.runtime.entities.display_color_window = true;
        let mut w = DisplayColorWindow;
        hp_app::testing::run_ctx(&mut s, |ctx, state| w.show(ctx, state));
        assert!(s.runtime.entities.display_color_window, "stays open");
        let mut panel = crate::EntityPanel::new();
        hp_app::testing::run_ui(&mut s, |ui, state| panel.ui(ui, state));
        s.doc.set_selection(&[ObjRef::Solid(3)]);
        s.settings.render.object_colors_2d = false;
        hp_app::testing::run_ui(&mut s, |ui, state| panel.ui(ui, state));
        assert_eq!(s.doc.history_len().0, steps, "drawing does not edit");
    }

    #[test]
    fn hex_and_window_closed_by_default() {
        assert_eq!(hex([1, 171, 255]), "#01ABFF");
        let mut s = hp_app::testing::test_state();
        let mut w = DisplayColorWindow;
        hp_app::testing::run_ctx(&mut s, |ctx, state| w.show(ctx, state));
        assert!(!s.runtime.entities.display_color_window);
    }
}
