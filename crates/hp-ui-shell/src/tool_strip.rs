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

pub const STRIP_WIDTH: f32 = strip_width(1);

pub const fn strip_width(columns: u8) -> f32 {
    let n = if columns >= 2 { 2.0 } else { 1.0 };
    n * BUTTON.x + (n - 1.0) * GAP + 2.0 * MARGIN_X as f32 + 1.0
}

pub fn palette_columns(workflow: &hp_app::settings::WorkflowSettings) -> u8 {
    if workflow.tool_palette_two_columns() { 2 } else { 1 }
}

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
        ToolId::Entity
        | ToolId::Block
        | ToolId::Polygon
        | ToolId::Curve
        | ToolId::Road
        | ToolId::Pipe
        | ToolId::Sprinkle => PaletteGroup::Create,
        ToolId::Texture
        | ToolId::Decal
        | ToolId::Overlay
        | ToolId::Clip
        | ToolId::Slice
        | ToolId::Extrude
        | ToolId::Bevel
        | ToolId::Vertex
        | ToolId::Displacement => PaletteGroup::Edit,
        ToolId::Path | ToolId::Cordon | ToolId::Physics => PaletteGroup::Special,
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
                out.push(PaletteItem::Action(Action::SendToBlender));
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StripRow {
    Line,
    Buttons(Vec<PaletteItem>),
}

pub fn strip_rows(items: &[PaletteItem], columns: u8) -> Vec<StripRow> {
    let columns = usize::from(columns.max(1));
    let mut out = Vec::new();
    let mut group: Vec<PaletteItem> = Vec::new();
    let flush = |group: &mut Vec<PaletteItem>, out: &mut Vec<StripRow>| {
        out.extend(group.chunks(columns).map(|c| StripRow::Buttons(c.to_vec())));
        group.clear();
    };
    for item in items {
        if *item == PaletteItem::Separator {
            flush(&mut group, &mut out);
            out.push(StripRow::Line);
        } else {
            group.push(*item);
        }
    }
    flush(&mut group, &mut out);
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
    let width = strip_width(palette_columns(&state.settings.workflow));
    let frame =
        egui::Frame::new().fill(strip_fill(ui.visuals())).inner_margin(egui::Margin::symmetric(MARGIN_X, MARGIN_Y));
    let r = egui::Panel::left(PANEL_ID)
        .resizable(false)
        .exact_size(width)
        .frame(frame)
        .show(ui, |ui| tool_palette(ui, state));
    Some(r.response.rect)
}

pub fn strip_fill(visuals: &egui::Visuals) -> Color32 {
    visuals.faint_bg_color
}

pub fn set_palette_columns(state: &mut AppState, columns: u8) {
    let columns = columns.clamp(1, 2);
    let w = &mut state.settings.workflow;
    if w.tool_palette_columns != columns {
        w.tool_palette_columns = columns;
        state.settings_dirty = true;
    }
}

fn palette_menu(ui: &mut Ui, state: &mut AppState) {
    let columns = palette_columns(&state.settings.workflow);
    for (n, key) in [(1, "shell.strip.one_column"), (2, "shell.strip.two_columns")] {
        if ui.radio(columns == n, t!(key)).clicked() {
            set_palette_columns(state, n);
            ui.close();
        }
    }
    ui.separator();
    if ui.button(t!("shell.strip.hide")).clicked() {
        state.settings.workflow.show_tool_palette = false;
        state.settings_dirty = true;
        ui.close();
    }
    crate::common::close_on_escape(ui);
}

pub fn tool_palette(ui: &mut Ui, state: &mut AppState) {
    let columns = palette_columns(&state.settings.workflow);
    let rows = strip_rows(&shown_items(), columns);
    let separators = rows.iter().filter(|r| **r == StripRow::Line).count();
    let button_rows = rows.len() - separators;
    let height = button_height(ui.available_height(), button_rows, separators);
    let gaps = GAP * f32::from(columns - 1);
    let width = ((ui.available_width() - gaps) / f32::from(columns)).clamp(1.0, BUTTON.x);
    let size = egui::vec2(width, height);
    let line_width = width * f32::from(columns) + gaps;
    let background = ui
        .scope_builder(egui::UiBuilder::new().sense(egui::Sense::click()), |ui| {
            egui::ScrollArea::vertical().id_salt("hp_tool_strip").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                let mut previous_button = false;
                for row in rows {
                    let StripRow::Buttons(items) = row else {
                        group_line(ui, line_width);
                        previous_button = false;
                        continue;
                    };
                    if previous_button {
                        ui.add_space(GAP);
                    }
                    previous_button = true;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(GAP, 0.0);
                        for item in items {
                            let r = match item {
                                PaletteItem::Tool(tool) => tool_button(ui, state, tool, size),
                                PaletteItem::Action(action) => action_button(ui, state, action, size),
                                PaletteItem::Isolate => isolate_button(ui, state, size),
                                PaletteItem::Separator => continue,
                            };
                            r.context_menu(|ui| palette_menu(ui, state));
                        }
                    });
                }
            });
        })
        .response;
    background.context_menu(|ui| palette_menu(ui, state));
}

fn group_line(ui: &mut Ui, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, SEPARATOR), egui::Sense::hover());
    if ui.is_rect_visible(rect) {
        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        let painter = ui.painter();
        painter.hline(rect.x_range().shrink(4.0), painter.round_to_pixel_center(rect.center().y), stroke);
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

fn icon(icon: crate::icons::Icon) -> impl FnOnce(&egui::Painter, egui::Pos2, Color32, f32) {
    move |painter, c, color, scale| crate::icons::paint(painter, c, color, icon, scale)
}

fn glyph(glyph: &'static str) -> impl FnOnce(&egui::Painter, egui::Pos2, Color32, f32) {
    move |painter, c, color, scale| {
        painter.text(c, egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(ICON_SIZE * scale), color);
    }
}

fn tool_button(ui: &mut Ui, state: &mut AppState, tool: ToolId, size: egui::Vec2) -> egui::Response {
    let on = state.tool == tool || state.requested_tool == Some(tool);
    let name = t!(tool.label_key());
    let r = strip_button(ui, size, on, true, name, icon(crate::icons::Icon::Tool(tool)));
    let r = r.on_hover_ui(|ui| {
        let tip = tooltip_with_shortcut(name, state.keys.shortcut(tool.action()));
        ui.label(RichText::new(tip).strong());
        ui.label(RichText::new(tool.action().label()).small().color(ui.visuals().weak_text_color()));
    });
    if r.clicked() {
        state.set_tool(tool);
    }
    r
}

fn action_button(ui: &mut Ui, state: &mut AppState, action: Action, size: egui::Vec2) -> egui::Response {
    let enabled = action_enabled(state, action);
    let r = match crate::icons::action_icon(action) {
        Some(i) => strip_button(ui, size, false, enabled, action.label(), icon(i)),
        None => strip_button(ui, size, false, enabled, action.label(), glyph(crate::toolbar::button_glyph(action))),
    };
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
    r
}

fn disabled_hint(action: Action) -> Option<&'static str> {
    match action {
        Action::BrushesToProp => Some(t!("shell.toolbar.propper_needs")),
        Action::PropToBrushes => Some(t!("shell.toolbar.unpropper_needs")),
        Action::SendToBlender => Some(t!("shell.toolbar.blender_needs")),
        Action::ApplyCurrentTexture => Some(t!("shell.toolbar.apply_texture_needs")),
        _ => None,
    }
}

fn isolate_button(ui: &mut Ui, state: &mut AppState, size: egui::Vec2) -> egui::Response {
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
    let r = strip_button(ui, size, isolated, enabled, tip, icon(crate::icons::Icon::Eye));
    let r = r.on_hover_ui(|ui| {
        ui.label(RichText::new(tooltip_with_shortcut(tip, state.keys.shortcut(action))).strong());
    });
    let r = r.on_disabled_hover_text(t!("shell.toolbar.isolate_none"));
    if r.clicked() {
        state.request(action);
    }
    r
}

pub(crate) const TR: hp_app::i18n::TrTable = &[
    ("shell.strip.one_column", "One column", "Une colonne"),
    ("shell.strip.two_columns", "Two columns", "Deux colonnes"),
    ("shell.strip.hide", "Hide the tool palette", "Masquer la palette d'outils"),
];

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
                T(ToolId::Road),
                T(ToolId::Pipe),
                S,
                T(ToolId::Texture),
                A(Action::ApplyCurrentTexture),
                T(ToolId::Decal),
                T(ToolId::Overlay),
                T(ToolId::Clip),
                T(ToolId::Slice),
                T(ToolId::Extrude),
                T(ToolId::Bevel),
                T(ToolId::Vertex),
                T(ToolId::Displacement),
                S,
                T(ToolId::Path),
                T(ToolId::Cordon),
                T(ToolId::Physics),
                A(Action::BrushesToProp),
                A(Action::PropToBrushes),
                A(Action::SendToBlender),
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
        assert!(!l.shapes.is_empty());
        let buttons = strip_buttons(&mut state, egui::vec2(1600.0, 960.0));
        for tool in ToolId::ALL {
            let name = t!(tool.label_key());
            assert!(buttons.iter().any(|b| b.0 == name && strip.contains(b.3.center())), "{tool:?} in the strip");
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
        let strip_x = 0.0..f64::from(strip_width(palette_columns(&state.settings.workflow)));
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
        let buttons = strip_buttons(&mut state, egui::vec2(1600.0, 1500.0));
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
            let mut out = ctx.run_ui(input, |ui| {
                tool_strip(ui, state);
            });
            out.textures_delta.clear();
            out.shapes
        };
        assert!(!frame(&mut state).is_empty());
        let buttons = strip_buttons(&mut state, egui::vec2(1600.0, 900.0));
        for action in [Action::BrushesToProp, Action::PropToBrushes] {
            assert!(buttons.iter().any(|b| b.0 == action.label()), "{action:?} is in the strip");
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

    #[test]
    fn rows_pair_the_buttons_of_each_group() {
        use PaletteItem::{Separator as S, Tool as T};
        let items = [T(ToolId::Select), T(ToolId::Camera), S, T(ToolId::Entity), T(ToolId::Block), T(ToolId::Polygon)];
        assert_eq!(
            strip_rows(&items, 2),
            vec![
                StripRow::Buttons(vec![T(ToolId::Select), T(ToolId::Camera)]),
                StripRow::Line,
                StripRow::Buttons(vec![T(ToolId::Entity), T(ToolId::Block)]),
                StripRow::Buttons(vec![T(ToolId::Polygon)]),
            ]
        );
        assert_eq!(strip_rows(&items, 1).len(), 6);
        assert_eq!(strip_rows(&items, 0), strip_rows(&items, 1));
        assert_eq!(STRIP_WIDTH, strip_width(1));
        assert_eq!(strip_width(2), STRIP_WIDTH + BUTTON.x + GAP);
        assert_eq!(strip_width(9), strip_width(2));
    }

    #[test]
    fn two_columns_make_a_wider_shorter_strip() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        set_palette_columns(&mut state, 2);
        assert!(state.settings_dirty && state.settings.workflow.tool_palette_two_columns());
        let ctx = egui::Context::default();
        let l = layout(&mut state, egui::vec2(1600.0, 900.0), &ctx);
        let strip = l.strip.expect("the strip is shown");
        assert!((strip.width() - strip_width(2)).abs() < 0.5, "{strip:?}");
        assert!(l.views.left() >= strip.right() - 0.5);
        let buttons = strip_buttons(&mut state, egui::vec2(1600.0, 900.0));
        assert_eq!(buttons.len(), shown_items().iter().filter(|i| **i != PaletteItem::Separator).count());
        let mut lefts: Vec<f32> = buttons.iter().map(|b| b.3.left()).collect();
        lefts.sort_by(f32::total_cmp);
        lefts.dedup();
        assert_eq!(lefts.len(), 2, "two columns: {lefts:?}");
        for b in &buttons {
            assert_eq!(b.3.size(), BUTTON, "{b:?} at full size: two columns leave room");
            assert!(strip.contains(b.3.center()), "{b:?}");
        }
        for tool in ToolId::ALL {
            assert!(buttons.iter().any(|b| b.0 == t!(tool.label_key())), "{tool:?}");
        }
        set_palette_columns(&mut state, 7);
        assert_eq!(state.settings.workflow.tool_palette_columns, 2);
        set_palette_columns(&mut state, 1);
        assert_eq!(strip_buttons(&mut state, egui::vec2(1600.0, 900.0)).len(), buttons.len());
    }

    #[test]
    fn right_click_offers_the_columns() {
        let _lang = hp_app::testing::lang_lock();
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let ctx = egui::Context::default();
        let mut shapes = Vec::new();
        let pos = egui::pos2(STRIP_WIDTH * 0.5, 30.0);
        for frame in 0..5 {
            let events = match frame {
                2 => [true, false]
                    .map(|pressed| egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    })
                    .into(),
                0 => vec![],
                _ => vec![egui::Event::PointerMoved(pos)],
            };
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                tool_strip(ui, &mut state);
            });
            out.textures_delta.clear();
            shapes = out.shapes;
        }
        for text in [t!("shell.strip.one_column"), t!("shell.strip.two_columns"), t!("shell.strip.hide")] {
            assert!(
                shapes.iter().any(|s| matches!(&s.shape, egui::Shape::Text(x) if x.galley.job.text == text)),
                "{text} in the menu"
            );
        }
        assert_eq!(state.tool, ToolId::Select, "a right click does not change the tool");
    }
}
