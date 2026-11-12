use egui::{
    Align2, Color32, CornerRadius, EventFilter, FontId, Id, Key, Modifiers, Popup, PopupCloseBehavior, Rect, Response,
    RichText, ScrollArea, Sense, Stroke, TextEdit, Ui, Vec2, pos2, vec2,
};
use hp_app::actions::key_label;
use hp_app::state::ThumbState;
use hp_app::{t, tf, theme};
use hp_editor::Id as EntityId;

use crate::edit::{ConnField, Op};
use crate::names::{filter_words, suggest};
use crate::view::Target;

pub(crate) const ROW_HEIGHT: f32 = 18.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConnText {
    Target,
    Param,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommitAction {
    SetKey { target: Target, key: String },
    SetNumber { target: Target, key: String, integer: bool },
    Targetname { target: Target },
    RenameKey { target: Target, old: String },
    Comments { ids: Vec<EntityId> },
    Conn { rows: Vec<Vec<(EntityId, usize)>>, field: ConnText },
}

impl CommitAction {
    pub fn into_op(self, text: String, update_references: bool) -> Option<Op> {
        match self {
            CommitAction::SetKey { target, key } => Some(Op::SetKey { target, key, value: text, merge: false }),
            CommitAction::SetNumber { target, key, integer } => {
                let valid =
                    if integer { crate::values::is_valid_integer(&text) } else { crate::values::is_valid_float(&text) };
                valid.then(|| Op::SetKey { target, key, value: text.trim().to_owned(), merge: false })
            }
            CommitAction::Targetname { target } => Some(Op::Targetname { target, value: text, update_references }),
            CommitAction::RenameKey { target, old } => Some(Op::RenameKey { target, old, new: text }),
            CommitAction::Comments { ids } => Some(Op::Comments { ids, text }),
            CommitAction::Conn { rows, field } => {
                let field = match field {
                    ConnText::Target => ConnField::Target(text),
                    ConnText::Param => ConnField::Param(text),
                };
                Some(Op::ConnEdit { rows, field, merge: false })
            }
        }
    }
}

struct ActiveEdit {
    id: Id,
    text: String,
    original: String,
    action: CommitAction,
    drawn: bool,
}

pub(crate) struct FieldOpts<'a> {
    pub hint: &'a str,
    pub width: f32,
    pub color: Option<Color32>,
    pub enabled: bool,
    pub multiline: bool,
}

impl Default for FieldOpts<'_> {
    fn default() -> Self {
        Self { hint: "", width: f32::INFINITY, color: None, enabled: true, multiline: false }
    }
}

#[derive(Default)]
pub(crate) struct TextBuffers {
    active: Option<ActiveEdit>,
    pending: Vec<(CommitAction, String)>,
}

impl TextBuffers {
    pub fn begin_frame(&mut self) {
        if let Some(a) = &mut self.active {
            a.drawn = false;
        }
    }

    pub fn end_frame(&mut self, ctx: &egui::Context) {
        let orphan = self.active.as_ref().is_some_and(|a| !a.drawn && !ctx.memory(|m| m.has_focus(a.id)));
        if orphan
            && let Some(a) = self.active.take()
            && a.text != a.original
            && !escape_pressed(ctx)
        {
            self.pending.push((a.action, a.text));
        }
    }

    pub fn take_commits(&mut self) -> Vec<(CommitAction, String)> {
        std::mem::take(&mut self.pending)
    }

    pub fn live_text(&self, id: Id) -> Option<&str> {
        self.active.as_ref().filter(|a| a.id == id).map(|a| a.text.as_str())
    }

    pub fn is_editing(&self, id: Id) -> bool {
        self.active.as_ref().is_some_and(|a| a.id == id)
    }

    pub fn discard(&mut self, id: Id) {
        if self.is_editing(id) {
            self.active = None;
        }
    }

    pub fn commit_now(&mut self, id: Id, text: String, action: CommitAction) {
        self.discard(id);
        self.pending.retain(|(a, _)| *a != action);
        self.pending.push((action, text));
    }

    pub fn field(
        &mut self,
        ui: &mut Ui,
        id: Id,
        display: &str,
        opts: FieldOpts<'_>,
        action: impl FnOnce() -> CommitAction,
    ) -> Response {
        let editing = self.is_editing(id);
        let mut text = match (&mut self.active, editing) {
            (Some(a), true) => std::mem::take(&mut a.text),
            _ => display.to_owned(),
        };
        let mut edit = if opts.multiline {
            TextEdit::multiline(&mut text).desired_rows(3)
        } else {
            TextEdit::singleline(&mut text)
        }
        .id(id)
        .desired_width(opts.width)
        .hint_text(opts.hint.to_owned());
        if let Some(c) = opts.color {
            edit = edit.text_color(c);
        }
        let resp = ui.add_enabled(opts.enabled, edit);
        match &mut self.active {
            Some(a) if editing => {
                a.text = text;
                a.drawn = true;
            }
            _ => {
                if resp.changed() {
                    self.active =
                        Some(ActiveEdit { id, text, original: display.to_owned(), action: action(), drawn: true });
                }
            }
        }
        let escape = escape_pressed(ui.ctx());
        let left = (resp.lost_focus() && self.is_editing(id))
            || (self.is_editing(id) && !resp.has_focus() && !ui.memory(|m| m.has_focus(id)));
        if left
            && let Some(a) = self.active.take()
            && !escape
            && a.text != a.original
        {
            self.pending.push((a.action, a.text));
        }
        resp
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PickerItem<'a> {
    pub name: &'a str,
    pub detail: &'a str,
}

#[derive(Clone, Debug, Default)]
struct PickerState {
    query: String,
    highlight: usize,
    focus: bool,
    scroll_to_highlight: bool,
    view: (f32, f32),
    cache_key: Option<(String, usize)>,
    filtered: Vec<usize>,
}

pub(crate) fn picker_button(ui: &mut Ui, text: impl Into<egui::WidgetText>, width: f32) -> Response {
    let text: egui::WidgetText = text.into();
    let button = egui::Button::new(text).truncate().right_text(icon("▾").weak());
    ui.add_sized(vec2(width, ui.spacing().interact_size.y), button)
}

pub(crate) fn picker_popup<'i>(
    button: &Response,
    allow_custom: bool,
    count: usize,
    item: impl Fn(usize) -> PickerItem<'i>,
) -> Option<String> {
    picker_popup_with(button, PickerOpts { allow_custom, ..PickerOpts::default() }, count, item)
}

#[derive(Default)]
pub(crate) struct PickerOpts<'a> {
    pub allow_custom: bool,
    pub search_details: bool,
    pub preview: Option<&'a dyn Fn(usize)>,
    pub preview_tip: &'a str,
    pub min_width: f32,
}

pub(crate) fn picker_popup_with<'i>(
    button: &Response,
    opts: PickerOpts<'_>,
    count: usize,
    item: impl Fn(usize) -> PickerItem<'i>,
) -> Option<String> {
    let allow_custom = opts.allow_custom;
    let ctx = button.ctx.clone();
    let popup_id = Popup::default_response_id(button);
    let state_id = popup_id.with("picker_state");
    if button.clicked() {
        ctx.data_mut(|d| d.insert_temp(state_id, PickerState { focus: true, ..PickerState::default() }));
    }
    let width = button.rect.width().max(280.0).max(opts.min_width);
    let mut picked: Option<String> = None;
    Popup::from_toggle_button_response(button)
        .close_behavior(PopupCloseBehavior::CloseOnClickOutside)
        .width(width)
        .show(|ui| {
            ui.set_min_width(width);
            let mut st: PickerState = ctx.data_mut(|d| d.remove_temp(state_id)).unwrap_or_default();
            let (down, up, page_down, page_up) = ui.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    i.consume_key(Modifiers::NONE, Key::ArrowUp),
                    i.consume_key(Modifiers::NONE, Key::PageDown),
                    i.consume_key(Modifiers::NONE, Key::PageUp),
                )
            });
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            let edit =
                ui.add(TextEdit::singleline(&mut st.query).hint_text(t!("common.search")).desired_width(f32::INFINITY));
            if st.focus {
                edit.request_focus();
                st.focus = false;
            }
            if edit.changed() {
                st.highlight = 0;
                st.scroll_to_highlight = true;
            }
            let key = (st.query.clone(), count);
            if st.cache_key.as_ref() != Some(&key) {
                let mut filtered = if opts.search_details {
                    filter_words_with_details(&st.query, (0..count).map(|i| (item(i).name, item(i).detail)))
                } else {
                    filter_words(&st.query, (0..count).map(|i| item(i).name))
                };
                let q = st.query.trim();
                if !q.is_empty()
                    && let Some(pos) = filtered.iter().position(|&i| item(i).name.eq_ignore_ascii_case(q))
                {
                    let exact = filtered.remove(pos);
                    filtered.insert(0, exact);
                }
                st.filtered = filtered;
                st.cache_key = Some(key);
            }
            let q = st.query.trim().to_owned();
            let n = st.filtered.len();
            if n > 0 {
                let step = |h: usize, d: isize| (h as isize + d).clamp(0, n as isize - 1) as usize;
                if down {
                    st.highlight = step(st.highlight, 1);
                }
                if up {
                    st.highlight = step(st.highlight, -1);
                }
                if page_down {
                    st.highlight = step(st.highlight, 10);
                }
                if page_up {
                    st.highlight = step(st.highlight, -10);
                }
                st.scroll_to_highlight |= down || up || page_down || page_up;
                st.highlight = st.highlight.min(n - 1);
            }
            if enter {
                picked = match st.filtered.get(st.highlight) {
                    Some(&i) => Some(item(i).name.to_owned()),
                    None if allow_custom && !q.is_empty() => Some(q.clone()),
                    None => None,
                };
            }
            if n == 0 {
                ui.add_space(4.0);
                if allow_custom && !q.is_empty() {
                    if ui.button(hp_app::tf!("entities.picker_use_custom", q)).clicked() {
                        picked = Some(q.clone());
                    }
                } else {
                    ui.label(RichText::new(t!("entities.no_match")).weak());
                }
            } else {
                let filtered = &st.filtered;
                let highlight = st.highlight;
                let screen = ui.ctx().content_rect();
                let below = screen.bottom() - button.rect.bottom();
                let above = button.rect.top() - screen.top();
                let max_h = (below.max(above) - 64.0).clamp(140.0, 320.0);
                let mut area = ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]);
                if std::mem::take(&mut st.scroll_to_highlight) {
                    let pitch = ROW_HEIGHT + ui.spacing().item_spacing.y;
                    let (offset, view_h) = st.view;
                    let view_h = if view_h > 0.0 { view_h } else { max_h };
                    if let Some(y) = row_scroll_offset(highlight, pitch, offset, view_h, false) {
                        area = area.vertical_scroll_offset(y);
                    }
                }
                let list = area.show_rows(ui, ROW_HEIGHT, n, |ui, range| {
                    for r in range {
                        let it = item(filtered[r]);
                        let look = RowLook {
                            main: it.name,
                            detail: it.detail,
                            selected: r == highlight,
                            ..RowLook::default()
                        };
                        let resp = match opts.preview {
                            Some(preview) => {
                                let (play, row) = play_row(ui, ui.scope_id().with(("picker_row", r)), look);
                                if play.on_hover_text(opts.preview_tip).clicked() {
                                    preview(filtered[r]);
                                }
                                row
                            }
                            None => list_row_look(ui, look, CLICK),
                        };
                        let resp = if it.detail.is_empty() { resp } else { resp.on_hover_text(it.detail) };
                        if resp.clicked() {
                            picked = Some(it.name.to_owned());
                        }
                    }
                });
                st.view = (list.state.clamped_offset().y, list.inner_rect.height());
                ui.label(RichText::new(hp_app::tf!("entities.picker_count", n)).small().weak());
            }
            if picked.is_some() {
                ui.close();
            }
            ctx.data_mut(|d| d.insert_temp(state_id, st));
        });
    picked
}

fn filter_words_with_details<'a>(query: &str, items: impl Iterator<Item = (&'a str, &'a str)>) -> Vec<usize> {
    let words: Vec<String> = query.split_whitespace().map(str::to_ascii_lowercase).collect();
    items
        .enumerate()
        .filter(|(_, (name, detail))| {
            words.is_empty() || {
                let (name, detail) = (name.to_ascii_lowercase(), detail.to_ascii_lowercase());
                words.iter().all(|w| name.contains(w.as_str()) || detail.contains(w.as_str()))
            }
        })
        .map(|(i, _)| i)
        .collect()
}

pub(crate) fn icon(glyph: &str) -> RichText {
    RichText::new(glyph).monospace()
}

pub(crate) fn search_box(ui: &mut Ui, text: &mut String, hint: &str) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let width = (ui.available_width() - SIDE_BUTTON_W - ui.spacing().item_spacing.x).max(60.0);
        let r = ui.add(TextEdit::singleline(text).hint_text(hint.to_owned()).desired_width(width));
        changed |= r.changed();
        if side_button(ui, "×", false).on_hover_text(t!("common.clear")).clicked() && !text.is_empty() {
            text.clear();
            changed = true;
        }
    });
    changed
}

pub(crate) const CLICK: Sense = Sense::CLICK;

pub(crate) fn side_button(ui: &mut Ui, text: impl Into<egui::WidgetText>, selected: bool) -> Response {
    let size = vec2(SIDE_BUTTON_W, ui.spacing().interact_size.y);
    let (rect, resp) = ui.allocate_exact_size(size, CLICK);
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact_selectable(&resp, selected);
        let fill = if selected { visuals.weak_bg_fill } else { ui.visuals().widgets.inactive.weak_bg_fill };
        let fill = if resp.hovered() && !selected { visuals.weak_bg_fill } else { fill };
        ui.painter().rect(rect, visuals.corner_radius, fill, visuals.bg_stroke, egui::StrokeKind::Inside);
        let galley =
            text.into().into_galley(ui, Some(egui::TextWrapMode::Truncate), rect.width(), egui::TextStyle::Button);
        let color = if ui.is_enabled() { visuals.text_color() } else { ui.visuals().weak_text_color() };
        let pos = rect.center() - galley.size() * 0.5;
        ui.painter().with_clip_rect(hp_app::widgets::clip_x(rect, ui.clip_rect())).galley(pos, galley, color);
    }
    resp
}

pub(crate) fn side_menu_button<R>(
    ui: &mut Ui,
    text: impl Into<egui::WidgetText>,
    content: impl FnOnce(&mut Ui) -> R,
) -> Response {
    let resp = side_button(ui, text, false);
    Popup::menu(&resp).show(content);
    resp
}

pub(crate) const SIDE_BUTTON_W: f32 = 22.0;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RowLook<'a> {
    pub glyph: &'a str,
    pub main: &'a str,
    pub detail: &'a str,
    pub selected: bool,
    pub strong: bool,
    pub indent: f32,
}

pub(crate) fn list_row(ui: &mut Ui, main: &str, detail: &str, selected: bool, strong: bool) -> Response {
    list_row_look(ui, RowLook { main, detail, selected, strong, ..RowLook::default() }, CLICK)
}

pub(crate) fn list_row_icon(
    ui: &mut Ui,
    glyph: &str,
    main: &str,
    detail: &str,
    selected: bool,
    strong: bool,
) -> Response {
    list_row_look(ui, RowLook { glyph, main, detail, selected, strong, indent: 0.0 }, CLICK)
}

pub(crate) fn list_row_look(ui: &mut Ui, look: RowLook<'_>, sense: Sense) -> Response {
    let width = ui.available_width().max(40.0);
    let sense = sense - Sense::FOCUSABLE;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), sense);
    paint_list_row(ui, rect.with_min_x((rect.left() + look.indent).min(rect.right())), resp.hovered(), look);
    resp
}

fn paint_list_row(ui: &Ui, rect: Rect, hovered: bool, look: RowLook<'_>) {
    if !ui.is_rect_visible(rect) {
        return;
    }
    let visuals = ui.visuals();
    if look.selected {
        ui.painter().rect_filled(rect, CornerRadius::same(2), visuals.selection.bg_fill);
    } else if hovered {
        ui.painter().rect_filled(rect, CornerRadius::same(2), visuals.widgets.hovered.weak_bg_fill);
    }
    let painter = ui.painter_at(rect);
    let font = FontId::proportional(13.0);
    let text_color = if look.selected || look.strong { visuals.strong_text_color() } else { visuals.text_color() };
    let mut x = rect.left() + 4.0;
    if !look.glyph.is_empty() {
        painter.text(
            pos2(x, rect.center().y),
            Align2::LEFT_CENTER,
            look.glyph,
            FontId::monospace(11.0),
            visuals.weak_text_color(),
        );
        x += 14.0;
    }
    let galley = painter.layout_no_wrap(look.main.to_owned(), font, text_color);
    let main_w = galley.size().x + (x - rect.left() - 4.0);
    let y = rect.center().y - galley.size().y * 0.5;
    painter.galley(pos2(x, y), galley, text_color);
    if !look.detail.is_empty() {
        let first_line = look.detail.lines().next().unwrap_or("");
        painter.text(
            pos2(rect.left() + 4.0 + main_w + 10.0, rect.center().y),
            Align2::LEFT_CENTER,
            first_line,
            FontId::proportional(11.5),
            visuals.weak_text_color(),
        );
    }
}

pub(crate) fn play_row(ui: &mut Ui, id: Id, look: RowLook<'_>) -> (Response, Response) {
    let width = ui.available_width().max(40.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
    let play_rect = Rect::from_min_size(rect.min, vec2(16.0, ROW_HEIGHT));
    let row_rect = rect.with_min_x(play_rect.right() + 2.0);
    let play = ui.interact(play_rect, id.with("play"), CLICK);
    let row = ui.interact(row_rect, id.with("row"), CLICK);
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let color = if play.hovered() { visuals.strong_text_color() } else { visuals.text_color() };
        ui.painter().text(play_rect.center(), Align2::CENTER_CENTER, "▶", FontId::monospace(10.0), color);
        paint_list_row(ui, row_rect, row.hovered(), look);
    }
    (play, row)
}

pub(crate) fn row_scroll_offset(row: usize, pitch: f32, offset: f32, view_h: f32, center: bool) -> Option<f32> {
    let top = row as f32 * pitch;
    let bottom = top + ROW_HEIGHT;
    if top >= offset && bottom <= offset + view_h {
        return None;
    }
    let y = if center {
        top - (view_h - ROW_HEIGHT) * 0.5
    } else if top < offset {
        top
    } else {
        bottom - view_h
    };
    Some(y.max(0.0))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompleteMode {
    Suggest,
    Complete,
}

#[derive(Clone, Debug, Default)]
struct CompleteState {
    highlight: Option<usize>,
    open: bool,
    shown: Vec<String>,
}

impl CompleteState {
    fn key_pick(&self, keys: CompleteKeys, mode: CompleteMode, list: &[String]) -> Option<String> {
        let index = if keys.enter {
            self.highlight
        } else if keys.tab {
            self.highlight.or((mode == CompleteMode::Complete).then_some(0))
        } else {
            None
        };
        index.and_then(|h| list.get(h)).cloned()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CompleteKeys {
    down: bool,
    up: bool,
    enter: bool,
    tab: bool,
}

pub(crate) fn autocomplete_keys(ui: &mut Ui, field_id: Id, mode: CompleteMode) -> CompleteKeys {
    let st_id = field_id.with("complete");
    let st = ui.data(|d| d.get_temp::<CompleteState>(st_id)).unwrap_or_default();
    if !st.open || !ui.memory(|m| m.has_focus(field_id)) {
        return CompleteKeys::default();
    }
    let holds_tab = mode == CompleteMode::Complete || st.highlight.is_some();
    if holds_tab && ui.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::Tab)) {
        ui.memory_mut(|m| m.move_focus(egui::FocusDirection::Previous));
    }
    ui.input_mut(|i| {
        let down = i.consume_key(Modifiers::NONE, Key::ArrowDown);
        let up = i.consume_key(Modifiers::NONE, Key::ArrowUp);
        let chosen = st.highlight.is_some() || down;
        let enter = chosen && i.consume_key(Modifiers::NONE, Key::Enter);
        let tab = (chosen || mode == CompleteMode::Complete) && i.consume_key(Modifiers::NONE, Key::Tab);
        CompleteKeys { down, up, enter, tab }
    })
}

pub(crate) fn autocomplete_popup(
    ui: &Ui,
    field: &Response,
    text: &str,
    candidates: &[&str],
    keys: CompleteKeys,
    mode: CompleteMode,
) -> Option<String> {
    let st_id = field.id.with("complete");
    let mut st: CompleteState = ui.data(|d| d.get_temp(st_id)).unwrap_or_default();
    if !field.has_focus() {
        let picked = st.key_pick(keys, mode, &st.shown);
        ui.data_mut(|d| d.insert_temp(st_id, CompleteState::default()));
        return picked;
    }
    let matches: Vec<usize> = suggest(text, candidates.iter().copied(), 12);
    let useful = !(matches.len() == 1 && candidates[matches[0]].eq_ignore_ascii_case(text.trim()));
    let shown: Vec<String> =
        if useful { matches.iter().map(|&i| candidates[i].to_owned()).collect() } else { Vec::new() };
    st.open = !shown.is_empty();
    if field.changed() {
        st.highlight = None;
    }
    let n = shown.len();
    if n == 0 {
        st.highlight = None;
    } else {
        if keys.down {
            st.highlight = Some(st.highlight.map_or(0, |h| (h + 1).min(n - 1)));
        }
        if keys.up {
            st.highlight = st.highlight.and_then(|h| h.checked_sub(1));
        }
        st.highlight = st.highlight.map(|h| h.min(n - 1));
    }
    let mut picked = st.key_pick(keys, mode, &shown);
    if st.open && picked.is_none() {
        let popup_id = field.id.with("complete_popup");
        Popup::new(popup_id, ui.ctx().clone(), field, field.layer_id)
            .width(field.rect.width().max(180.0))
            .close_behavior(PopupCloseBehavior::IgnoreClicks)
            .show(|ui| {
                ui.set_min_width(field.rect.width().max(180.0));
                for (row, name) in shown.iter().enumerate() {
                    if list_row(ui, name, "", st.highlight == Some(row), false).clicked() {
                        picked = Some(name.clone());
                    }
                }
                let (up, down, enter) = (key_label(Key::ArrowUp), key_label(Key::ArrowDown), key_label(Key::Enter));
                let hint = match mode {
                    CompleteMode::Suggest => tf!("entities.complete_hint", up, down, enter),
                    CompleteMode::Complete => tf!("entities.complete_hint_tab", key_label(Key::Tab), up, down, enter),
                };
                ui.label(RichText::new(hint).small().weak());
            });
    }
    if picked.is_some() {
        st.open = false;
        st.highlight = None;
        st.shown.clear();
    } else {
        st.shown = shown;
        if st.open && (mode == CompleteMode::Complete || st.highlight.is_some()) {
            let filter = EventFilter { tab: true, horizontal_arrows: true, vertical_arrows: true, escape: false };
            ui.memory_mut(|m| m.set_focus_lock_filter(field.id, filter));
            hp_app::dialog_keys::keep_tab_for(ui.ctx(), field.id);
        }
    }
    ui.data_mut(|d| d.insert_temp(st_id, st));
    picked
}

pub(crate) fn escape_pressed(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.raw.events.iter().any(|e| matches!(e, egui::Event::Key { key: Key::Escape, pressed: true, .. })))
}

pub(crate) const YAW_SNAP_DEG: f64 = 15.0;

pub(crate) fn yaw_dial(ui: &mut Ui, yaw_deg: Option<f64>, enabled: bool) -> (Response, Option<f64>) {
    let size = ui.spacing().interact_size.y + 2.0;
    let sense = if enabled { CLICK | Sense::DRAG } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), sense);
    let mut out = None;
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let d = p - rect.center();
        let snap = if ui.input(|i| i.modifiers.shift) { Some(YAW_SNAP_DEG) } else { None };
        out = crate::values::yaw_from_screen_dir(d.x, d.y, snap);
    }
    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&resp);
        let c = rect.center();
        let r = size * 0.5 - 1.5;
        ui.painter().circle_filled(c, r, ui.visuals().extreme_bg_color);
        ui.painter().circle_stroke(c, r, visuals.bg_stroke);
        ui.painter().line_segment([pos2(c.x, c.y - r), pos2(c.x, c.y - r + 3.0)], Stroke::new(1.0, theme::text_weak()));
        if let Some(yaw) = out.or(yaw_deg) {
            let a = yaw.to_radians() as f32;
            let tip = c + vec2(a.cos(), -a.sin()) * (r - 2.0);
            let color = if enabled { theme::accent() } else { ui.visuals().weak_text_color() };
            ui.painter().line_segment([c, tip], Stroke::new(2.0, color));
            ui.painter().circle_filled(tip, 2.0, color);
        }
    }
    (resp, out)
}

pub(crate) fn tab_bar<T: PartialEq + Copy>(ui: &mut Ui, current: &mut T, tabs: &[(T, String, bool)]) -> bool {
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (tab, label, warn) in tabs {
            let mut text = RichText::new(label.as_str());
            if *warn {
                text = text.color(theme::warning());
            }
            if ui.add(egui::Button::selectable(*current == *tab, text)).clicked() && *current != *tab {
                *current = *tab;
                changed = true;
            }
        }
    });
    changed
}

pub(crate) fn thumbnail(ui: &mut Ui, state: Option<ThumbState>, size: f32) -> Response {
    match state {
        Some(ThumbState::Ready { texture, size: px }) => {
            let (w, h) = (px[0].max(1) as f32, px[1].max(1) as f32);
            let scale = size / w.max(h);
            let img = egui::Image::new((texture, vec2(w * scale, h * scale)));
            ui.add_sized(vec2(size, size), img)
        }
        Some(ThumbState::Missing) => {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(2),
                Stroke::new(1.0, theme::error()),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "?", FontId::proportional(12.0), theme::error());
            resp.on_hover_text(t!("entities.material_missing"))
        }
        _ => {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(2), ui.visuals().extreme_bg_color);
            resp
        }
    }
}

pub(crate) fn content_width(ui: &Ui) -> f32 {
    let scroll = &ui.spacing().scroll;
    let reserve = if scroll.floating { scroll.bar_width + scroll.bar_outer_margin } else { 0.0 };
    (ui.available_width() - reserve).max(0.0)
}

pub(crate) fn fit_scroll_content(ui: &mut Ui) {
    let w = content_width(ui);
    ui.set_max_width(w);
}

pub(crate) fn column_widths(total: f32, fractions: &[f32], min: f32) -> Vec<f32> {
    let n = fractions.len();
    if n == 0 {
        return Vec::new();
    }
    let total = total.max(0.0);
    let min = min.max(0.0).min(total / n as f32);
    let fractions: Vec<f32> = fractions.iter().map(|f| f.max(0.0)).collect();
    let mut at_min = vec![false; n];
    loop {
        let fixed = at_min.iter().filter(|m| **m).count();
        let rest = total - min * fixed as f32;
        let sum: f32 = fractions.iter().zip(&at_min).filter(|(_, m)| !**m).map(|(f, _)| *f).sum();
        if sum <= 1e-6 {
            let free = n - fixed;
            return at_min.iter().map(|m| if *m || free == 0 { min } else { rest / free as f32 }).collect();
        }
        let mut changed = false;
        for (i, f) in fractions.iter().enumerate() {
            if !at_min[i] && rest * f / sum < min {
                at_min[i] = true;
                changed = true;
            }
        }
        if !changed {
            return fractions.iter().zip(&at_min).map(|(f, m)| if *m { min } else { rest * f / sum }).collect();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SortBy {
    pub column: usize,
    pub ascending: bool,
}

pub(crate) fn table_header(ui: &mut Ui, labels: &[&str], widths: &[f32], sort: Option<&mut SortBy>) -> bool {
    let mut changed = false;
    let mut sort = sort;
    let total: f32 = widths.iter().sum();
    let (rect, _) = ui.allocate_exact_size(vec2(total, ROW_HEIGHT + 2.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::ZERO, ui.visuals().faint_bg_color);
    let mut x = rect.left();
    for (i, (label, w)) in labels.iter().zip(widths).enumerate() {
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(*w, rect.height()));
        let resp = ui.interact(cell, ui.scope_id().with(("table_header", i)), CLICK);
        let color = if resp.hovered() { ui.visuals().strong_text_color() } else { ui.visuals().text_color() };
        let painter = ui.painter().with_clip_rect(hp_app::widgets::clip_x(cell.shrink(1.0), ui.clip_rect()));
        painter.text(
            pos2(cell.left() + 4.0, cell.center().y),
            Align2::LEFT_CENTER,
            *label,
            FontId::proportional(12.5),
            color,
        );
        if let Some(s) = sort.as_deref()
            && s.column == i
        {
            let arrow = if s.ascending { "▲" } else { "▼" };
            painter.text(
                pos2(cell.right() - 4.0, cell.center().y),
                Align2::RIGHT_CENTER,
                arrow,
                FontId::monospace(9.0),
                theme::accent(),
            );
        }
        if let Some(s) = sort.as_deref_mut()
            && resp.clicked()
        {
            if s.column == i {
                s.ascending = !s.ascending;
            } else {
                *s = SortBy { column: i, ascending: true };
            }
            changed = true;
        }
        x += w;
    }
    changed
}

pub(crate) fn table_row(
    ui: &mut Ui,
    cells: &[(&str, Option<Color32>)],
    widths: &[f32],
    selected: bool,
    striped: bool,
) -> Response {
    let total: f32 = widths.iter().sum::<f32>().max(ui.available_width());
    let (rect, resp) = ui.allocate_exact_size(vec2(total, ROW_HEIGHT), CLICK);
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let visuals = ui.visuals();
    if selected {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, visuals.selection.bg_fill);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, visuals.widgets.hovered.weak_bg_fill);
    } else if striped {
        ui.painter().rect_filled(rect, CornerRadius::ZERO, stripe_color(ui));
    }
    let mut x = rect.left();
    for ((text, color), w) in cells.iter().zip(widths) {
        let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(*w, rect.height()));
        ui.painter().with_clip_rect(hp_app::widgets::clip_x(cell.shrink2(vec2(1.0, 0.0)), ui.clip_rect())).text(
            pos2(cell.left() + 4.0, cell.center().y),
            Align2::LEFT_CENTER,
            *text,
            FontId::proportional(12.5),
            color.unwrap_or(if selected { visuals.strong_text_color() } else { visuals.text_color() }),
        );
        x += w;
    }
    resp
}

pub(crate) fn stripe_color(ui: &Ui) -> Color32 {
    ui.visuals().faint_bg_color.gamma_multiply(0.45)
}

pub(crate) fn empty_note(ui: &mut Ui, text: &str) {
    ui.add_space(12.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(text).weak());
    });
    ui.add_space(12.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picker_search_in_details() {
        let items = [("Streetwar.d3_c17_13_horn", "ambient/alarms/klaxon1.wav"), ("ambient/alarms/klaxon1.wav", "")];
        assert_eq!(filter_words_with_details("klaxon", items.iter().copied()), [0, 1]);
        assert_eq!(filter_words_with_details("streetwar klaxon", items.iter().copied()), [0]);
        assert_eq!(filter_words_with_details("", items.iter().copied()), [0, 1]);
        assert!(filter_words_with_details("siren", items.iter().copied()).is_empty());
    }

    #[test]
    fn widths_respect_minimum() {
        let w = column_widths(100.0, &[1.0, 1.0, 2.0], 30.0);
        assert_eq!(w, [30.0, 30.0, 40.0]);
        let w = column_widths(400.0, &[1.0, 3.0], 10.0);
        assert_eq!(w, [100.0, 300.0]);
    }

    #[test]
    fn widths_never_exceed_the_total() {
        for total in [80.0, 150.0, 250.0, 320.0, 333.3, 600.0] {
            let w = column_widths(total, &[1.2, 1.2, 1.1, 0.8, 0.45], 36.0);
            let sum: f32 = w.iter().sum();
            assert!((sum - total).abs() < 0.01, "{total}: {w:?}");
            let min = 36.0f32.min(total / 5.0);
            assert!(w.iter().all(|c| *c >= min - 0.01), "{total}: {w:?}");
        }
        assert_eq!(column_widths(100.0, &[0.0, 0.0], 10.0), [50.0, 50.0]);
        assert!(column_widths(100.0, &[], 10.0).is_empty());
        assert_eq!(column_widths(-5.0, &[1.0], 10.0), [0.0]);
    }

    #[test]
    fn commit_actions_map_to_ops() {
        let target = Target::Entities(vec![1]);
        let op = CommitAction::SetKey { target: target.clone(), key: "k".into() }.into_op("v".into(), true);
        assert_eq!(op, Some(Op::SetKey { target: target.clone(), key: "k".into(), value: "v".into(), merge: false }));
        let op = CommitAction::Targetname { target: target.clone() }.into_op("n".into(), false);
        assert_eq!(op, Some(Op::Targetname { target, value: "n".into(), update_references: false }));
        let op = CommitAction::Conn { rows: vec![vec![(1, 0)]], field: ConnText::Param }.into_op("p".into(), true);
        assert_eq!(
            op,
            Some(Op::ConnEdit { rows: vec![vec![(1, 0)]], field: ConnField::Param("p".into()), merge: false })
        );
        let bad = CommitAction::SetNumber { target: Target::World, key: "k".into(), integer: true };
        assert_eq!(bad.into_op("1.5".into(), true), None);
        let good = CommitAction::SetNumber { target: Target::World, key: "k".into(), integer: false };
        assert!(good.into_op(" 1.5 ".into(), true).is_some());
    }

    #[test]
    fn text_buffers_commit_orphans() {
        let ctx = egui::Context::default();
        let mut buffers = TextBuffers::default();
        let id = Id::unique("field");
        let action = CommitAction::SetKey { target: Target::World, key: "k".into() };
        buffers.active =
            Some(ActiveEdit { id, text: "new".into(), original: "old".into(), action: action.clone(), drawn: true });
        assert_eq!(buffers.live_text(id), Some("new"));
        buffers.begin_frame();
        buffers.end_frame(&ctx);
        let commits = buffers.take_commits();
        assert_eq!(commits, vec![(action.clone(), "new".to_owned())]);
        assert!(!buffers.is_editing(id));
        buffers.commit_now(id, "x".into(), action);
        assert_eq!(buffers.take_commits().len(), 1);
    }
}
