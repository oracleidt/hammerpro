use egui::{Color32, RichText, Ui};
use hp_app::{Action, AppState, ToolId, t};

use crate::common::{action_enabled, tooltip_with_shortcut};

const BUTTON: egui::Vec2 = egui::vec2(34.0, 32.0);
const MIN_BUTTON_HEIGHT: f32 = 26.0;
const GAP: f32 = 2.0;
const SEPARATOR: f32 = 11.0;
const ICON_SIZE: f32 = 18.0;
const MARGIN_X: i8 = 5;
const MARGIN_Y: i8 = 6;
const RADIUS: f32 = 5.0;

pub const STRIP_WIDTH: f32 = BUTTON.x + 2.0 * MARGIN_X as f32 + 1.0;

pub const PANEL_ID: &str = "hp_tools";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PaletteGroup {
    Navigate,
    Create,
    Edit,
    Special,
    View,
}

impl PaletteGroup {
    pub const ALL: [PaletteGroup; 5] =
        [PaletteGroup::Navigate, PaletteGroup::Create, PaletteGroup::Edit, PaletteGroup::Special, PaletteGroup::View];
}

pub fn tool_group(tool: ToolId) -> PaletteGroup {
    match tool {
        ToolId::Select | ToolId::Camera => PaletteGroup::Navigate,
        ToolId::Entity | ToolId::Block | ToolId::Polygon | ToolId::Curve | ToolId::Sprinkle => PaletteGroup::Create,
        ToolId::Texture
        | ToolId::Decal
        | ToolId::Overlay
        | ToolId::Clip
        | ToolId::Extrude
        | ToolId::Bevel
        | ToolId::Vertex
        | ToolId::Displacement => PaletteGroup::Edit,
        ToolId::Path | ToolId::Cordon => PaletteGroup::Special,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteItem {
    Tool(ToolId),
    Action(Action),
    Isolate,
    Separator,
}

pub fn palette_items() -> Vec<PaletteItem> {
    let mut out = Vec::new();
    for group in PaletteGroup::ALL {
        if !out.is_empty() {
            out.push(PaletteItem::Separator);
        }
        for tool in ToolId::ALL.into_iter().filter(|t| tool_group(*t) == group) {
            out.push(PaletteItem::Tool(tool));
            if tool == ToolId::Texture {
                out.push(PaletteItem::Action(Action::ApplyCurrentTexture));
            }
        }
        match group {
            PaletteGroup::Special => {
                out.push(PaletteItem::Action(Action::BrushesToProp));
                out.push(PaletteItem::Action(Action::PropToBrushes));
            }
            PaletteGroup::View => out.push(PaletteItem::Isolate),
            _ => {}
        }
    }
    out
}

fn shown_items() -> Vec<PaletteItem> {
    let mut out: Vec<PaletteItem> = Vec::new();
    for item in palette_items() {
        match item {
            PaletteItem::Tool(tool) if !tool.is_ready() => {}
            PaletteItem::Action(action) if !action.is_ready() => {}
            PaletteItem::Separator if out.last().is_none_or(|l| *l == PaletteItem::Separator) => {}
            other => out.push(other),
        }
    }
    if out.last() == Some(&PaletteItem::Separator) {
        out.pop();
    }
    out
}

pub fn button_height(available: f32, buttons: usize, separators: usize) -> f32 {
    if buttons == 0 {
        return BUTTON.y;
    }
    let fixed = separators as f32 * SEPARATOR + buttons.saturating_sub(1 + separators) as f32 * GAP;
    ((available - fixed) / buttons as f32).clamp(MIN_BUTTON_HEIGHT, BUTTON.y)
}

pub fn tool_strip(ui: &mut Ui, state: &mut AppState) -> Option<egui::Rect> {
    if !state.settings.workflow.show_tool_palette {
        return None;
    }
    let frame =
        egui::Frame::new().fill(strip_fill(ui.visuals())).inner_margin(egui::Margin::symmetric(MARGIN_X, MARGIN_Y));
    let r = egui::Panel::left(PANEL_ID)
        .resizable(false)
        .exact_size(STRIP_WIDTH)
        .frame(frame)
        .show(ui, |ui| tool_palette(ui, state));
    Some(r.response.rect)
}

pub fn strip_fill(visuals: &egui::Visuals) -> Color32 {
    visuals.faint_bg_color
}

pub fn tool_palette(ui: &mut Ui, state: &mut AppState) {
    let items = shown_items();
    let separators = items.iter().filter(|i| **i == PaletteItem::Separator).count();
    let buttons = items.len() - separators;
    let height = button_height(ui.available_height(), buttons, separators);
    let size = egui::vec2(BUTTON.x.min(ui.available_width()).max(1.0), height);
    egui::ScrollArea::vertical().id_salt("hp_tool_strip").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        let mut previous_button = false;
        for item in items {
            if item == PaletteItem::Separator {
                group_line(ui, size.x);
                previous_button = false;
                continue;
            }
            if previous_button {
                ui.add_space(GAP);
            }
            previous_button = true;
            match item {
                PaletteItem::Tool(tool) => tool_button(ui, state, tool, size),
                PaletteItem::Action(action) => action_button(ui, state, action, size),
                PaletteItem::Isolate => isolate_button(ui, state, size),
                PaletteItem::Separator => {}
            }
        }
    });
}

fn group_line(ui: &mut Ui, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, SEPARATOR), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        ui.painter().hline(rect.x_range().shrink(4.0), rect.center().y, stroke);
    }
}

fn strip_button(
    ui: &mut Ui,
    size: egui::Vec2,
    on: bool,
    enabled: bool,
    name: &str,
    paint: impl FnOnce(&egui::Painter, egui::Pos2, Color32, f32),
) -> egui::Response {
    ui.scope(|ui| {
        if !enabled {
            ui.disable();
        }
        let (rect, r) = ui.allocate_exact_size(size, egui::Sense::click());
        r.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, name));
        if ui.is_rect_visible(rect) {
            let visuals = ui.visuals();
            let painter = ui.painter();
            let hovered = enabled && r.hovered();
            let fill = if on {
                hp_app::theme::accent_bg()
            } else if enabled && r.is_pointer_button_down_on() {
                visuals.widgets.active.weak_bg_fill
            } else if hovered {
                visuals.widgets.hovered.weak_bg_fill
            } else {
                Color32::TRANSPARENT
            };
            painter.rect_filled(rect, RADIUS, fill);
            if hovered && !on {
                painter.rect_stroke(rect, RADIUS, visuals.widgets.hovered.bg_stroke, egui::StrokeKind::Inside);
            }
            if r.has_focus() {
                painter.rect_stroke(
                    rect.expand(1.0),
                    RADIUS + 1.0,
                    visuals.selection.stroke,
                    egui::StrokeKind::Outside,
                );
            }
            let color = if !enabled {
                visuals.weak_text_color().gamma_multiply(0.55)
            } else if on || hovered {
                visuals.strong_text_color()
            } else {
                visuals.text_color()
            };
            let scale = (size.y / BUTTON.y).clamp(0.8, 1.0);
            paint(painter, rect.center(), color, scale);
        }
        r
    })
    .inner
}

fn glyph(glyph: &'static str) -> impl FnOnce(&egui::Painter, egui::Pos2, Color32, f32) {
    move |painter, c, color, scale| {
        painter.text(c, egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(ICON_SIZE * scale), color);
    }
}

fn tool_button(ui: &mut Ui, state: &mut AppState, tool: ToolId, size: egui::Vec2) {
    let on = state.tool == tool || state.requested_tool == Some(tool);
    let name = t!(tool.label_key());
    let r = strip_button(ui, size, on, true, name, glyph(crate::common::tool_icon(tool)));
    let r = r.on_hover_ui(|ui| {
        let tip = tooltip_with_shortcut(name, state.keys.shortcut(tool.action()));
        ui.label(RichText::new(tip).strong());
        ui.label(RichText::new(tool.action().label()).small().color(ui.visuals().weak_text_color()));
    });
    if r.clicked() {
        state.set_tool(tool);
    }
}

fn action_button(ui: &mut Ui, state: &mut AppState, action: Action, size: egui::Vec2) {
    let enabled = action_enabled(state, action);
    let r = strip_button(ui, size, false, enabled, action.label(), glyph(crate::toolbar::button_glyph(action)));
    let tip = crate::common::action_tooltip(state, action);
    let r = r.on_hover_ui(|ui| {
        ui.label(RichText::new(&tip).strong());
    });
    let r = r.on_disabled_hover_ui(|ui| {
        ui.label(RichText::new(&tip).strong());
        if let Some(why) = disabled_hint(action) {
            ui.label(RichText::new(why).small().color(ui.visuals().weak_text_color()));
        }
    });
    if r.clicked() {
        state.request(action);
    }
}

fn disabled_hint(action: Action) -> Option<&'static str> {
    match action {
        Action::BrushesToProp => Some(t!("shell.toolbar.propper_needs")),
        Action::PropToBrushes => Some(t!("shell.toolbar.unpropper_needs")),
        Action::ApplyCurrentTexture => Some(t!("shell.toolbar.apply_texture_needs")),
        _ => None,
    }
}

fn isolate_button(ui: &mut Ui, state: &mut AppState, size: egui::Vec2) {
    let id = ui.scope_id().with("hp_shell_isolate");
    let revision = state.doc.revision();
    let cached: Option<(u64, bool)> = ui.data(|d| d.get_temp(id));
    let isolated = match cached {
        Some((r, hidden)) if r == revision => hidden,
        _ => {
            let hidden = state.doc.any_hidden();
            ui.data_mut(|d| d.insert_temp(id, (revision, hidden)));
            hidden
        }
    };
    let action = if isolated { Action::UnhideAll } else { Action::HideUnselected };
    let enabled = isolated || !state.doc.selected_objects().is_empty();
    let tip = if isolated { t!("shell.toolbar.isolate_off") } else { t!("shell.toolbar.isolate_on") };
    let r = strip_button(ui, size, isolated, enabled, tip, |painter, c, color, scale| {
        crate::toolbar::paint_eye(painter, c, color, scale);
    });
    let r = r.on_hover_ui(|ui| {
        ui.label(RichText::new(tooltip_with_shortcut(tip, state.keys.shortcut(action))).strong());
    });
    let r = r.on_disabled_hover_text(t!("shell.toolbar.isolate_none"));
    if r.clicked() {
        state.request(action);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_groups_keep_hammer_order() {
        use PaletteItem::{Action as A, Isolate, Separator as S, Tool as T};
        assert_eq!(
            palette_items(),
            vec![
                T(ToolId::Select),
                T(ToolId::Camera),
                S,
                T(ToolId::Entity),
                T(ToolId::Block),
                T(ToolId::Polygon),
                T(ToolId::Curve),
                T(ToolId::Sprinkle),
                S,
                T(ToolId::Texture),
                A(Action::ApplyCurrentTexture),
                T(ToolId::Decal),
                T(ToolId::Overlay),
                T(ToolId::Clip),
                T(ToolId::Extrude),
                T(ToolId::Bevel),
                T(ToolId::Vertex),
                T(ToolId::Displacement),
                S,
                T(ToolId::Path),
                T(ToolId::Cordon),
                A(Action::BrushesToProp),
                A(Action::PropToBrushes),
                S,
                Isolate,
            ]
        );
        for tool in ToolId::ALL {
            assert_eq!(palette_items().iter().filter(|i| **i == T(tool)).count(), 1, "{tool:?}");
        }
        let hammer = [
            T(ToolId::Select),
            T(ToolId::Camera),
            T(ToolId::Entity),
            T(ToolId::Block),
            T(ToolId::Texture),
            A(Action::ApplyCurrentTexture),
            T(ToolId::Decal),
            T(ToolId::Overlay),
            T(ToolId::Clip),
            T(ToolId::Vertex),
        ];
        let items = palette_items();
        let pos: Vec<usize> = hammer.iter().map(|h| items.iter().position(|i| i == h).unwrap()).collect();
        assert!(pos.windows(2).all(|w| w[0] < w[1]), "{pos:?}");
        let shown = shown_items();
        assert_ne!(shown.first(), Some(&S));
        assert_ne!(shown.last(), Some(&S));
        assert!(shown.windows(2).all(|w| !(w[0] == S && w[1] == S)));
    }

    #[test]
    fn buttons_flatten_then_stop() {
        assert_eq!(button_height(2000.0, 18, 4), BUTTON.y, "room enough: full height");
        let h = button_height(560.0, 18, 4);
        assert!(h < BUTTON.y && h > MIN_BUTTON_HEIGHT, "{h}");
        assert_eq!(button_height(100.0, 18, 4), MIN_BUTTON_HEIGHT, "no room: the strip scrolls");
        assert_eq!(button_height(100.0, 0, 0), BUTTON.y);
    }

    struct Layout {
        top: egui::Rect,
        status: egui::Rect,
        strip: Option<egui::Rect>,
        console: egui::Rect,
        tabs: egui::Rect,
        views: egui::Rect,
        shapes: Vec<egui::epaint::ClippedShape>,
    }

    fn layout(state: &mut AppState, size: egui::Vec2, ctx: &egui::Context) -> Layout {
        let mut frame = || {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let mut rects = None;
            let mut full = ctx.run_ui(input, |ui| {
                let top = egui::Panel::top("top").show(ui, |ui| crate::toolbar::toolbar(ui, state)).response.rect;
                let status = egui::Panel::bottom("status").exact_size(22.0).show(ui, |_| {}).response.rect;
                let strip = tool_strip(ui, state);
                let console = egui::Panel::bottom("console").exact_size(150.0).show(ui, |_| {}).response.rect;
                let tabs = egui::Panel::top("tabs").exact_size(24.0).show(ui, |_| {}).response.rect;
                let views = egui::CentralPanel::no_frame().show(ui, |_| {}).response.rect;
                rects = Some((top, status, strip, console, tabs, views));
            });
            full.textures_delta.clear();
            let (top, status, strip, console, tabs, views) = rects.expect("the frame ran");
            Layout { top, status, strip, console, tabs, views, shapes: full.shapes }
        };
        frame();
        frame()
    }

    #[test]
    fn strip_is_its_own_panel_left_of_the_views() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        let l = layout(&mut state, egui::vec2(1600.0, 960.0), &ctx);
        let strip = l.strip.expect("the strip is shown");
        assert_eq!(strip.left(), 0.0, "flush with the window edge");
        assert!((strip.top() - l.top.bottom()).abs() < 1.0, "{strip:?} under {:?}", l.top);
        assert!((strip.bottom() - l.status.top()).abs() < 1.0, "{strip:?} above {:?}", l.status);
        assert!((strip.width() - STRIP_WIDTH).abs() < 0.5, "{strip:?}");
        for (what, r) in [("console", l.console), ("map tabs", l.tabs), ("views", l.views)] {
            assert!(r.left() >= strip.right() - 0.5, "{what} {r:?} right of the strip {strip:?}");
        }
        for tool in ToolId::ALL {
            let icon = crate::common::tool_icon(tool);
            assert!(find_text_in(&l.shapes, icon, strip).is_some(), "{tool:?} in the strip");
        }
        state.settings.workflow.show_tool_palette = false;
        let l = layout(&mut state, egui::vec2(1600.0, 960.0), &ctx);
        assert!(l.strip.is_none());
        assert_eq!(l.views.left(), 0.0);
        assert_eq!(l.console.left(), 0.0);
    }

    fn strip_buttons(state: &mut AppState, size: egui::Vec2) -> Vec<(String, bool, bool, egui::Rect)> {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut tree = None;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                egui::Panel::top("top").exact_size(95.0).show(ui, |_| {});
                egui::Panel::bottom("status").exact_size(22.0).show(ui, |_| {});
                tool_strip(ui, state);
                egui::CentralPanel::no_frame().show(ui, |_| {});
            });
            out.textures_delta.clear();
            tree = out.platform_output.accesskit_update;
        }
        let tree = tree.expect("an accessibility tree");
        let strip_x = 0.0..STRIP_WIDTH as f64;
        let mut out: Vec<(String, bool, bool, egui::Rect)> = tree
            .nodes
            .iter()
            .map(|(_, n)| n)
            .filter(|n| n.role() == egui::Role::Button)
            .filter_map(|n| {
                let b = n.bounds()?;
                strip_x.contains(&b.x0).then(|| {
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(b.x0 as f32, b.y0 as f32),
                        egui::pos2(b.x1 as f32, b.y1 as f32),
                    );
                    let on = n.toggled() == Some(egui::accesskit::Toggled::True);
                    (n.label().unwrap_or_default().to_owned(), on, n.is_disabled(), rect)
                })
            })
            .collect();
        out.sort_by(|a, b| a.3.top().total_cmp(&b.3.top()));
        out
    }

    #[test]
    fn small_window_keeps_every_button() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        let l = layout(&mut state, egui::vec2(960.0, 600.0), &ctx);
        assert!(l.strip.expect("the strip is shown").height() < 560.0);
        let buttons = strip_buttons(&mut state, egui::vec2(960.0, 600.0));
        for tool in ToolId::ALL {
            let name = t!(tool.label_key());
            let b = buttons.iter().find(|b| b.0 == name).unwrap_or_else(|| panic!("{name} in {buttons:?}"));
            assert!(b.3.height() >= MIN_BUTTON_HEIGHT - 0.5 && b.3.height() < BUTTON.y, "{b:?}");
        }
        assert_eq!(buttons.len(), shown_items().iter().filter(|i| **i != PaletteItem::Separator).count());
    }

    #[test]
    fn buttons_have_accessible_names() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.tool = ToolId::Block;
        let buttons = strip_buttons(&mut state, egui::vec2(1600.0, 960.0));
        let button = |label: &str| buttons.iter().find(|b| b.0 == label).unwrap_or_else(|| panic!("{label}")).clone();
        assert!(button(t!("tool.block")).1, "the active tool is toggled");
        assert!(!button(t!("tool.select")).1);
        assert!(button(Action::BrushesToProp.label()).2, "Propper is disabled with nothing selected");
        let select = button(t!("tool.select")).3;
        assert_eq!(select.size(), BUTTON);
        assert!(buttons.windows(2).all(|w| w[0].3.left() == w[1].3.left() && w[0].3.top() < w[1].3.top()));
    }

    #[test]
    fn propper_buttons_are_in_the_strip() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        let frame = |state: &mut AppState| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| {
                tool_strip(ui, state);
            });
            out.shapes
        };
        let shapes = frame(&mut state);
        for glyph in ["🗿", "🔳"] {
            assert!(find_text_in(&shapes, glyph, egui::Rect::EVERYTHING).is_some(), "{glyph} is in the strip");
        }
        assert!(!action_enabled(&state, Action::BrushesToProp), "nothing selected");
        assert!(!action_enabled(&state, Action::PropToBrushes), "no prop selected");
        state.doc.select_all();
        assert!(action_enabled(&state, Action::BrushesToProp), "brushes selected");
        frame(&mut state);
        let menus = crate::menu::menu_actions();
        assert!(menus.contains(&Action::BrushesToProp) && menus.contains(&Action::PropToBrushes));
        assert!(state.requested_actions.is_empty());
    }

    fn find_text_in(shapes: &[egui::epaint::ClippedShape], text: &str, area: egui::Rect) -> Option<egui::Rect> {
        shapes.iter().find_map(|s| match &s.shape {
            egui::Shape::Text(t) if t.galley.job.text == text && area.contains(t.visual_bounding_rect().center()) => {
                Some(t.visual_bounding_rect())
            }
            _ => None,
        })
    }
}
