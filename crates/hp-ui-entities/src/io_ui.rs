use std::collections::BTreeSet;

use egui::{DragValue, Event, Id, Popup, RichText, ScrollArea, Ui, vec2};
use hp_app::actions::{Action, KeyBindings, clipboard_event_shortcut, key_label, shortcut_from_event};
use hp_app::{t, tf, theme};
use hp_editor::Id as EntityId;
use hp_fgd::{IoDef, ResolvedFgd};
use hp_vmf::Connection;

use crate::cx::Cx;
use crate::edit::{ConnField, Op};
use crate::names::{MapNames, SPECIAL_TARGETS};
use crate::pick::{ConnRows, ViewPicks, conn_key};
use crate::smartedit::{PICK_GLYPH, picker_is_open};
use crate::values::fmt_num_rounded;
use crate::view::{Agg, ConnIssues, ConnRow, EntityView, Target};
use crate::widgets::{
    CommitAction, CompleteMode, ConnText, FieldOpts, PickerItem, ROW_HEIGHT, SIDE_BUTTON_W, TextBuffers,
    autocomplete_keys, autocomplete_popup, column_widths, empty_note, icon, picker_button, picker_popup, side_button,
    table_header, table_row,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IoAction {
    GoTo(EntityId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableCommand {
    Delete,
    Copy,
    Cut,
    Paste,
    SelectAll,
}

impl TableCommand {
    fn of(action: Action) -> Option<TableCommand> {
        Some(match action {
            Action::Delete => TableCommand::Delete,
            Action::Copy => TableCommand::Copy,
            Action::Cut => TableCommand::Cut,
            Action::Paste => TableCommand::Paste,
            Action::SelectAll => TableCommand::SelectAll,
            _ => return None,
        })
    }
}

fn take_table_commands(ui: &Ui, keys: &KeyBindings, wanted: impl Fn(TableCommand) -> bool) -> Vec<TableCommand> {
    ui.input_mut(|i| {
        let mut out = Vec::new();
        i.events.retain(|event| {
            let shortcut = shortcut_from_event(event).or_else(|| clipboard_event_shortcut(event));
            let Some(command) = shortcut.and_then(|s| keys.action_for(&s)).and_then(TableCommand::of) else {
                return true;
            };
            if !wanted(command) {
                return true;
            }
            if !matches!(event, Event::Key { repeat: true, .. }) {
                out.push(command);
            }
            false
        });
        out
    })
}

pub(crate) struct IoEditor {
    salt: Id,
    selected: BTreeSet<usize>,
    anchor: Option<usize>,
    clipboard: Vec<Connection>,
    select_last: Option<usize>,
    pub buffers: TextBuffers,
    input_selected: Option<usize>,
}

impl IoEditor {
    pub fn new() -> IoEditor {
        IoEditor {
            salt: Id::unique("hp_ui_entities.io"),
            selected: BTreeSet::new(),
            anchor: None,
            clipboard: Vec::new(),
            select_last: None,
            buffers: TextBuffers::default(),
            input_selected: None,
        }
    }

    pub fn reset_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
        self.input_selected = None;
        self.select_last = None;
    }

    #[cfg(test)]
    pub fn select_rows(&mut self, rows: &[usize]) {
        self.selected = rows.iter().copied().collect();
        self.anchor = rows.first().copied();
    }

    #[cfg(test)]
    pub fn selected_rows(&self) -> Vec<usize> {
        self.selected.iter().copied().collect()
    }

    #[cfg(test)]
    pub fn target_field_id(&self) -> Id {
        self.salt.with("target_field")
    }

    pub fn outputs(
        &mut self,
        ui: &mut Ui,
        cx: &mut Cx,
        picks: &mut ViewPicks,
        view: &EntityView,
        names: &MapNames,
        ops: &mut Vec<Op>,
    ) {
        let Target::Entities(ids) = &view.target else {
            empty_note(ui, t!("entities.world_no_outputs"));
            return;
        };
        let n = view.conns.len();
        self.selected.retain(|&i| i < n);
        if let Some(previous) = self.select_last
            && n > previous
        {
            self.selected.clear();
            self.selected.insert(n - 1);
            self.anchor = Some(n - 1);
            self.select_last = None;
        }

        let area = ui.available_rect_before_wrap();
        let mut insert = false;
        if ui.rect_contains_pointer(area) && !ui.ctx().text_edit_focused() && !Popup::is_any_open(ui.ctx()) {
            let has_sel = !self.selected.is_empty();
            let can_paste = has_sel || !self.clipboard.is_empty();
            let commands = take_table_commands(ui, cx.keys, |c| match c {
                TableCommand::Delete | TableCommand::Copy | TableCommand::Cut => has_sel,
                TableCommand::Paste => can_paste,
                TableCommand::SelectAll => n > 0,
            });
            for command in commands {
                self.run(ui.ctx(), command, view, ids, ops);
            }
            insert = ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Insert));
        }
        let esc = cx.settings.entities.io_esc_delimiter;

        ui.horizontal_wrapped(|ui| {
            let add = ui.button(format!("+ {}", t!("common.add")));
            let add =
                add.on_hover_text(format!("{} ({})", t!("entities.add_output_tip"), key_label(egui::Key::Insert)));
            if add.clicked() || insert {
                let output = default_output(cx.fgd, view.single_class());
                ops.push(Op::ConnAdd { ids: ids.clone(), conn: new_connection(output, esc) });
                self.select_last = Some(n);
            }
            let has_sel = !self.selected.is_empty();
            let tip = with_shortcut(t!("entities.copy_outputs"), cx.keys, Action::Copy);
            if ui.add_enabled(has_sel, egui::Button::new(t!("action.copy"))).on_hover_text(tip).clicked() {
                self.run(ui.ctx(), TableCommand::Copy, view, ids, ops);
            }
            let paste_tip = with_shortcut(&tf!("entities.paste_outputs", self.clipboard.len()), cx.keys, Action::Paste);
            if ui
                .add_enabled(!self.clipboard.is_empty(), egui::Button::new(t!("action.paste")))
                .on_hover_text(paste_tip)
                .clicked()
            {
                self.run(ui.ctx(), TableCommand::Paste, view, ids, ops);
            }
            let tip = with_shortcut(t!("entities.delete_outputs"), cx.keys, Action::Delete);
            if ui.add_enabled(has_sel, egui::Button::new(t!("action.delete"))).on_hover_text(tip).clicked() {
                self.run(ui.ctx(), TableCommand::Delete, view, ids, ops);
            }
            let can_mark = self.selected.iter().filter_map(|&i| view.conns.get(i)).any(|r| {
                let t = r.conn.target.trim();
                !t.is_empty() && !t.starts_with('!') && names.target_exists(t)
            });
            let mark_btn = ui.add_enabled(can_mark, egui::Button::new(t!("entities.mark")));
            let mark_btn = mark_btn
                .on_hover_ui(|ui| {
                    ui.label(tf!("entities.mark_outputs_tip", self.selected_targets(cx, view).len()));
                })
                .on_disabled_hover_text(t!("entities.mark_none"));
            if mark_btn.clicked() {
                let targets = self.selected_targets(cx, view);
                crate::pick::mark(cx.doc, cx.requested_actions, &targets);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let invalid = view.conns.iter().filter(|r| r.issues.any()).count();
                if invalid > 0 {
                    ui.label(RichText::new(tf!("entities.outputs_invalid", invalid)).color(theme::warning()));
                }
            });
        });
        if view.conns_hidden > 0 {
            ui.label(RichText::new(tf!("entities.outputs_hidden", view.conns_hidden)).color(theme::warning()).small());
        }

        let total = ui.available_width();
        let mut widths = column_widths(total - 20.0, &[1.2, 1.2, 1.1, 0.8, 0.45], 36.0);
        widths.insert(0, 20.0);
        let headers = [
            "",
            t!("entities.col_output"),
            t!("entities.col_target"),
            t!("entities.col_input"),
            t!("entities.col_param"),
            t!("entities.col_delay"),
        ];
        table_header(ui, &headers, &widths, None);
        let table_h = (ui.available_height() * 0.45).clamp(ROW_HEIGHT * 4.0, ROW_HEIGHT * 14.0);
        if n == 0 {
            empty_note(ui, t!("entities.no_outputs"));
        } else {
            ScrollArea::vertical()
                .id_salt(self.salt.with("outputs"))
                .max_height(table_h)
                .auto_shrink([false, true])
                .show_rows(ui, ROW_HEIGHT, n, |ui, range| {
                    for i in range {
                        let row = &view.conns[i];
                        let resp = conn_row(ui, row, &widths, self.selected.contains(&i), i % 2 == 1);
                        let resp = resp.on_hover_ui(|ui| conn_tooltip(ui, &row.conn, &row.issues));
                        if resp.clicked() {
                            let mods = ui.input(|inp| inp.modifiers);
                            self.click_row(i, mods);
                        }
                    }
                });
        }
        ui.separator();
        self.output_editor(ui, cx, picks, view, names, ops);
    }

    fn selected_targets(&self, cx: &Cx, view: &EntityView) -> Vec<EntityId> {
        let mut out: Vec<EntityId> = Vec::new();
        for &i in &self.selected {
            let Some(row) = view.conns.get(i) else { continue };
            for id in crate::pick::entities_matching_target(cx.doc, &row.conn.target) {
                if !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        out
    }

    fn run(
        &mut self,
        ctx: &egui::Context,
        command: TableCommand,
        view: &EntityView,
        ids: &[EntityId],
        ops: &mut Vec<Op>,
    ) {
        match command {
            TableCommand::Copy | TableCommand::Cut => {
                let copied: Vec<Connection> =
                    self.selected.iter().filter_map(|&i| view.conns.get(i)).map(|r| r.conn.clone()).collect();
                if copied.is_empty() {
                    return;
                }
                let text: String =
                    copied.iter().map(|c| format!("\"{}\" \"{}\"\n", c.output, c.to_value_string())).collect();
                ctx.copy_text(text);
                self.clipboard = copied;
                if command == TableCommand::Cut {
                    self.run(ctx, TableCommand::Delete, view, ids, ops);
                }
            }
            TableCommand::Paste => {
                if !self.clipboard.is_empty() {
                    ops.push(Op::ConnPaste { ids: ids.to_vec(), conns: self.clipboard.clone() });
                }
            }
            TableCommand::Delete => {
                let rows: Vec<Vec<(EntityId, usize)>> =
                    self.selected.iter().filter_map(|&i| view.conns.get(i)).map(|r| r.refs.clone()).collect();
                if !rows.is_empty() {
                    ops.push(Op::ConnDelete { rows });
                }
                self.selected.clear();
                self.anchor = None;
            }
            TableCommand::SelectAll => {
                self.selected = (0..view.conns.len()).collect();
                self.anchor = (!self.selected.is_empty()).then_some(0);
            }
        }
    }

    fn click_row(&mut self, i: usize, mods: egui::Modifiers) {
        if mods.shift
            && let Some(a) = self.anchor
        {
            let (lo, hi) = if a <= i { (a, i) } else { (i, a) };
            if !mods.command {
                self.selected.clear();
            }
            self.selected.extend(lo..=hi);
        } else if mods.command {
            if !self.selected.remove(&i) {
                self.selected.insert(i);
            }
            self.anchor = Some(i);
        } else {
            self.selected.clear();
            self.selected.insert(i);
            self.anchor = Some(i);
        }
    }

    fn output_editor(
        &mut self,
        ui: &mut Ui,
        cx: &mut Cx,
        picks: &mut ViewPicks,
        view: &EntityView,
        names: &MapNames,
        ops: &mut Vec<Op>,
    ) {
        let rows: Vec<&ConnRow> = self.selected.iter().filter_map(|&i| view.conns.get(i)).collect();
        if rows.is_empty() {
            ui.label(RichText::new(t!("entities.select_output_hint")).weak());
            return;
        }
        let refs: Vec<Vec<(EntityId, usize)>> = rows.iter().map(|r| r.refs.clone()).collect();
        let output = Agg::of(rows.iter().map(|r| Some(r.conn.output.as_str())));
        let target = Agg::of(rows.iter().map(|r| Some(r.conn.target.as_str())));
        let input = Agg::of(rows.iter().map(|r| Some(r.conn.input.as_str())));
        let param = Agg::of(rows.iter().map(|r| Some(r.conn.param.as_str())));
        let first_delay = rows[0].conn.delay;
        let delay_same = rows.iter().all(|r| (r.conn.delay - first_delay).abs() < 1e-6);
        let first_times = rows[0].conn.times;
        let times_same = rows.iter().all(|r| r.conn.times == first_times);
        let multiple = t!("common.multiple_values");
        let fgd = cx.fgd;
        let full = ui.available_width();
        let label_w = (full * 0.42).clamp(40.0, 190.0);
        let field_w = (full - label_w - 8.0).max(20.0);
        let pick_w = (field_w - SIDE_BUTTON_W - 4.0).max(20.0);
        let param_takes_name = input.value().is_some_and(|i| {
            input_defs(fgd, names, target.value().unwrap_or(""))
                .iter()
                .find(|d| d.name.eq_ignore_ascii_case(i))
                .is_some_and(|d| takes_entity_name(&d.arg_type))
        });

        let grid = egui::Grid::new(self.salt.with("editor")).num_columns(2).min_col_width(0.0);
        grid.spacing(vec2(6.0, 4.0)).show(ui, |ui| {
            ui.add_sized(vec2(label_w, 18.0), egui::Label::new(t!("entities.my_output")).truncate());
            let text = output.value().unwrap_or(multiple);
            let unknown_output = rows.iter().any(|r| r.issues.unknown_output);
            let rich = if unknown_output { RichText::new(text).color(theme::error()) } else { RichText::new(text) };
            let btn = picker_button(ui, rich, field_w);
            if picker_is_open(&btn) {
                let defs = output_defs(fgd, view.single_class());
                if let Some(o) = picker_popup(&btn, true, defs.len(), |i| io_item(defs.get(i).copied())) {
                    ops.push(Op::ConnEdit { rows: refs.clone(), field: ConnField::Output(o), merge: false });
                }
            }
            ui.end_row();

            ui.add_sized(vec2(label_w, 18.0), egui::Label::new(t!("entities.target_entities")).truncate());
            let id = self.salt.with("target_field");
            let keys = autocomplete_keys(ui, id, CompleteMode::Complete);
            let display = target.text();
            let live = self.buffers.live_text(id).unwrap_or(display).to_owned();
            let exists = live.trim().is_empty() || names.target_exists(&live);
            let color = (!exists).then_some(theme::error());
            let hint = if target.is_mixed() { multiple } else { "" };
            let action = || CommitAction::Conn { rows: refs.clone(), field: ConnText::Target };
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let resp = self.buffers.field(
                    ui,
                    id,
                    display,
                    FieldOpts { hint, width: pick_w, color, ..FieldOpts::default() },
                    action,
                );
                let resp = if exists { resp } else { resp.on_hover_text(t!("entities.target_missing")) };
                if resp.has_focus() || resp.lost_focus() {
                    let typed = self.buffers.live_text(id).unwrap_or(display).to_owned();
                    let mut candidates: Vec<&str> = SPECIAL_TARGETS.to_vec();
                    candidates.extend(names.names().iter().map(String::as_str));
                    if let Some(pick) = autocomplete_popup(ui, &resp, &typed, &candidates, keys, CompleteMode::Complete)
                    {
                        self.buffers.commit_now(id, pick, action());
                        ui.memory_mut(|m| m.surrender_focus(id));
                    }
                }
                conn_pick_button(ui, cx, picks, &refs, ConnText::Target);
            });
            ui.end_row();

            ui.add_sized(vec2(label_w, 18.0), egui::Label::new(t!("entities.via_input")).truncate());
            let text = input.value().unwrap_or(multiple);
            let unknown_input = rows.iter().any(|r| r.issues.unknown_input);
            let rich = if unknown_input { RichText::new(text).color(theme::error()) } else { RichText::new(text) };
            let btn = picker_button(ui, rich, field_w);
            if picker_is_open(&btn) {
                let defs = input_defs(fgd, names, target.value().unwrap_or(""));
                if let Some(i) = picker_popup(&btn, true, defs.len(), |i| io_item(defs.get(i).copied())) {
                    ops.push(Op::ConnEdit { rows: refs.clone(), field: ConnField::Input(i), merge: false });
                }
            }
            ui.end_row();

            ui.add_sized(vec2(label_w, 18.0), egui::Label::new(t!("entities.param_override")).truncate());
            let id = self.salt.with("param_field");
            let hint = if param.is_mixed() { multiple } else { t!("entities.param_none") };
            let live = self.buffers.live_text(id).unwrap_or(param.text()).to_owned();
            let quoted = live.contains('"');
            let color = quoted.then_some(theme::error());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let width = if param_takes_name { pick_w } else { field_w };
                self.buffers.field(
                    ui,
                    id,
                    param.text(),
                    FieldOpts { hint, width, color, ..FieldOpts::default() },
                    || CommitAction::Conn { rows: refs.clone(), field: ConnText::Param },
                );
                if param_takes_name {
                    conn_pick_button(ui, cx, picks, &refs, ConnText::Param);
                } else {
                    picks.cancel(cx.pick, conn_key(ConnText::Param));
                }
            });
            ui.end_row();
            if quoted {
                ui.label("");
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(RichText::new(t!("entities.param_quote")).color(theme::warning()).small())
                            .wrap(),
                    );
                    let script = input.value().is_some_and(|i| i.eq_ignore_ascii_case("RunScriptCode"));
                    if script && ui.small_button(t!("entities.param_backticks")).clicked() {
                        self.buffers.commit_now(
                            id,
                            live.replace('"', "`"),
                            CommitAction::Conn { rows: refs.clone(), field: ConnText::Param },
                        );
                        ui.memory_mut(|m| m.surrender_focus(id));
                    }
                });
                ui.end_row();
            }

            ui.add_sized(vec2(label_w, 18.0), egui::Label::new(t!("entities.delay_seconds")).truncate());
            let mut delay = first_delay;
            let dv = DragValue::new(&mut delay).speed(0.05).range(0.0..=3600.0).max_decimals(3).suffix(" s");
            let resp = ui.add(dv);
            let resp = if delay_same { resp } else { resp.on_hover_text(multiple) };
            if resp.changed() {
                ops.push(Op::ConnEdit { rows: refs.clone(), field: ConnField::Delay(delay), merge: true });
            }
            ui.end_row();

            ui.add_sized(vec2(label_w, 18.0), egui::Label::new(t!("entities.refires")).truncate());
            let mut times = first_times.clamp(-1, crate::edit::MAX_TIMES);
            let always = t!("entities.refire_always");
            let dv = DragValue::new(&mut times)
                .speed(0.1)
                .range(-1..=crate::edit::MAX_TIMES)
                .custom_formatter(move |v, _| if v < 0.0 { always.to_owned() } else { format!("{v:.0}") })
                .custom_parser(|s| {
                    let s = s.trim();
                    if s.is_empty() || s.starts_with('-') || s.eq_ignore_ascii_case(always) {
                        Some(-1.0)
                    } else {
                        s.parse::<f64>().ok().filter(|v| v.is_finite()).map(f64::round)
                    }
                });
            let resp = ui.add(dv).on_hover_text(t!("entities.refires_tip"));
            let resp = if times_same { resp } else { resp.on_hover_text(multiple) };
            if resp.changed() {
                let times = if times == 0 { if first_times > 0 { -1 } else { 1 } } else { times };
                ops.push(Op::ConnEdit { rows: refs.clone(), field: ConnField::Times(times), merge: true });
            }
            ui.end_row();
        });
    }

    pub fn inputs(&mut self, ui: &mut Ui, view: &EntityView) -> Option<IoAction> {
        if view.target.is_world() {
            empty_note(ui, t!("entities.world_no_outputs"));
            return None;
        }
        if view.names.is_empty() {
            empty_note(ui, t!("entities.no_name_no_inputs"));
            return None;
        }
        let n = view.inputs.len();
        ui.label(RichText::new(tf!("entities.inputs_count", n)).weak());
        if n == 0 {
            empty_note(ui, t!("entities.no_inputs"));
            return None;
        }
        let total = ui.available_width();
        let widths = column_widths(total, &[1.3, 1.1, 1.1, 0.8, 0.45], 36.0);
        let headers = [
            t!("entities.col_source"),
            t!("entities.col_output"),
            t!("entities.col_input"),
            t!("entities.col_param"),
            t!("entities.col_delay"),
        ];
        table_header(ui, &headers, &widths, None);
        let mut action = None;
        let table_h = (ui.available_height() - 30.0).max(ROW_HEIGHT * 4.0);
        ScrollArea::vertical()
            .id_salt(self.salt.with("inputs"))
            .max_height(table_h)
            .auto_shrink([false, true])
            .show_rows(ui, ROW_HEIGHT, n, |ui, range| {
                for i in range {
                    let row = &view.inputs[i];
                    let source =
                        if row.source_name.is_empty() { row.source_class.clone() } else { row.source_name.clone() };
                    let delay = fmt_num_rounded(f64::from(row.conn.delay), 3);
                    let cells = [
                        (source.as_str(), None),
                        (row.conn.output.as_str(), None),
                        (row.conn.input.as_str(), None),
                        (row.conn.param.as_str(), None),
                        (delay.as_str(), None),
                    ];
                    let resp = table_row(ui, &cells, &widths, self.input_selected == Some(i), i % 2 == 1);
                    let resp = resp.on_hover_ui(|ui| {
                        ui.label(RichText::new(format!("{} ({})", source, row.source_class)).strong());
                        ui.label(
                            RichText::new(format!("{} → {}.{}", row.conn.output, row.conn.target, row.conn.input))
                                .monospace(),
                        );
                        if !row.source_is_world {
                            ui.label(RichText::new(t!("entities.goto_tip")).weak());
                        }
                    });
                    if resp.clicked() {
                        self.input_selected = Some(i);
                    }
                    if resp.double_clicked() && !row.source_is_world {
                        action = Some(IoAction::GoTo(row.source));
                    }
                }
            });
        let selected = self.input_selected.and_then(|i| view.inputs.get(i));
        let can_go = selected.is_some_and(|r| !r.source_is_world);
        if ui.add_enabled(can_go, egui::Button::new(t!("entities.goto_source"))).clicked()
            && let Some(r) = selected
        {
            action = Some(IoAction::GoTo(r.source));
        }
        action
    }
}

fn takes_entity_name(kind: &hp_fgd::PropertyType) -> bool {
    use hp_fgd::PropertyType as P;
    matches!(kind, P::TargetDestination | P::TargetNameOrClass | P::TargetSource)
}

pub(crate) fn conn_pick_button(ui: &mut Ui, cx: &mut Cx, picks: &mut ViewPicks, rows: &ConnRows, field: ConnText) {
    let key = conn_key(field);
    let waiting = picks.is_waiting(cx.pick, key);
    let resp = side_button(ui, icon(PICK_GLYPH), waiting).on_hover_text(t!("entities.pick_entity"));
    if resp.clicked() {
        if waiting {
            picks.cancel(cx.pick, key);
        } else {
            picks.start_conn(cx.pick, rows.clone(), field);
        }
    }
}

fn with_shortcut(tip: &str, keys: &KeyBindings, action: Action) -> String {
    match keys.shortcut(action) {
        Some(s) => format!("{tip} ({})", hp_app::actions::shortcut_label(&s)),
        None => tip.to_owned(),
    }
}

pub(crate) fn new_connection(output: String, esc: bool) -> Connection {
    Connection { output, separator: if esc { hp_vmf::CONNECTION_ESC } else { ',' }, ..Connection::default() }
}

pub(crate) fn default_output(fgd: Option<&ResolvedFgd>, class: Option<&str>) -> String {
    class
        .and_then(|c| fgd?.class(c))
        .and_then(|c| c.outputs.iter().find(|o| o.name.eq_ignore_ascii_case("OnTrigger")).or(c.outputs.first()))
        .map_or_else(|| "OnUser1".to_owned(), |o| o.name.clone())
}

pub(crate) fn output_defs<'f>(fgd: Option<&'f ResolvedFgd>, class: Option<&str>) -> Vec<&'f IoDef> {
    let Some(fgd) = fgd else { return Vec::new() };
    match class.and_then(|c| fgd.class(c)) {
        Some(c) => {
            let mut v: Vec<&IoDef> = c.outputs.iter().collect();
            v.sort_by(|a, b| crate::names::cmp_ignore_case(&a.name, &b.name));
            v
        }
        None => fgd.all_outputs(),
    }
}

pub(crate) fn input_defs<'f>(fgd: Option<&'f ResolvedFgd>, names: &MapNames, target: &str) -> Vec<&'f IoDef> {
    let Some(fgd) = fgd else { return Vec::new() };
    let mut out: Vec<&IoDef> = Vec::new();
    for class in names.target_classes(target).iter().filter_map(|c| fgd.class(c)) {
        for def in &class.inputs {
            if !out.iter().any(|d| d.name.eq_ignore_ascii_case(&def.name)) {
                out.push(def);
            }
        }
    }
    if out.is_empty() {
        return fgd.all_inputs();
    }
    out.sort_by(|a, b| crate::names::cmp_ignore_case(&a.name, &b.name));
    out
}

fn io_item(def: Option<&IoDef>) -> PickerItem<'_> {
    match def {
        Some(d) => PickerItem { name: &d.name, detail: &d.description },
        None => PickerItem { name: "", detail: "" },
    }
}

fn conn_row(ui: &mut Ui, row: &ConnRow, widths: &[f32], selected: bool, striped: bool) -> egui::Response {
    let c = &row.conn;
    let err = Some(theme::error());
    let icon = if row.issues.any() {
        "!".to_owned()
    } else if c.times > 0 {
        c.times.to_string()
    } else {
        String::new()
    };
    let delay = fmt_num_rounded(f64::from(c.delay), 3);
    let cells = [
        (icon.as_str(), row.issues.any().then_some(theme::warning())),
        (c.output.as_str(), if row.issues.unknown_output { err } else { None }),
        (c.target.as_str(), if row.issues.missing_target { err } else { None }),
        (c.input.as_str(), if row.issues.unknown_input { err } else { None }),
        (c.param.as_str(), None),
        (delay.as_str(), None),
    ];
    table_row(ui, &cells, widths, selected, striped)
}

fn conn_tooltip(ui: &mut Ui, c: &Connection, issues: &ConnIssues) {
    ui.label(RichText::new(format!("{} → {}.{}", c.output, c.target, c.input)).monospace().strong());
    if !c.param.is_empty() {
        ui.label(tf!("entities.tip_param", c.param));
    }
    ui.label(tf!("entities.tip_delay", fmt_num_rounded(f64::from(c.delay), 3)));
    if c.times == 1 {
        ui.label(t!("entities.fire_once"));
    } else if c.times > 1 {
        ui.label(tf!("entities.tip_refires", c.times));
    }
    if issues.unknown_output {
        ui.colored_label(theme::error(), t!("entities.issue_output"));
    }
    if issues.missing_target {
        ui.colored_label(theme::error(), t!("entities.issue_target"));
    }
    if issues.unknown_input {
        ui.colored_label(theme::error(), t!("entities.issue_input"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fgd::test_fgd;

    #[test]
    fn io_lists_from_fgd() {
        let state = hp_app::testing::test_state();
        let fgd = test_fgd();
        let names = MapNames::build(&state.doc);
        assert_eq!(default_output(Some(&fgd), Some("trigger_multiple")), "OnTrigger");
        assert_eq!(default_output(Some(&fgd), Some("light")), "OnUser1");
        assert_eq!(default_output(None, None), "OnUser1");
        let outs: Vec<&str> =
            output_defs(Some(&fgd), Some("trigger_multiple")).iter().map(|d| d.name.as_str()).collect();
        assert_eq!(outs, ["OnStartTouch", "OnTrigger", "OnUser1"]);
        let ins: Vec<&str> = input_defs(Some(&fgd), &names, "main_light").iter().map(|d| d.name.as_str()).collect();
        assert_eq!(ins, ["Kill", "Toggle", "TurnOff", "TurnOn"]);
        let all = input_defs(Some(&fgd), &names, "!activator");
        assert!(all.len() > 4, "unresolved targets offer every input");
        assert!(input_defs(None, &names, "main_light").is_empty());
    }

    #[test]
    fn row_selection_with_modifiers() {
        let mut io = IoEditor::new();
        io.click_row(2, egui::Modifiers::NONE);
        io.click_row(5, egui::Modifiers::SHIFT);
        assert_eq!(io.selected.iter().copied().collect::<Vec<_>>(), [2, 3, 4, 5]);
        io.click_row(3, egui::Modifiers::COMMAND);
        assert_eq!(io.selected.iter().copied().collect::<Vec<_>>(), [2, 4, 5]);
        io.click_row(0, egui::Modifiers::NONE);
        assert_eq!(io.selected.iter().copied().collect::<Vec<_>>(), [0]);
        io.reset_selection();
        assert!(io.selected.is_empty());
    }
}
