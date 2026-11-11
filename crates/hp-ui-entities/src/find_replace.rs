use std::time::Instant;

use egui::{Id, Key, RichText, ScrollArea, Ui};
use hp_app::{AppState, Severity, t, tf, theme};
use hp_editor::{ConnPart, Id as EntityId, TextField, TextMatch, TextQuery};

use crate::view::{DocStamp, SelectionSummary};
use crate::widgets::{ROW_HEIGHT, column_widths, empty_note, table_header, table_row};

type SearchKey = (DocStamp, String, bool, bool, bool, bool, bool, Vec<EntityId>);

pub(crate) struct FindReplace {
    find: String,
    replace: String,
    whole_word: bool,
    case_sensitive: bool,
    key_names: bool,
    selection_only: bool,
    visible_only: bool,
    matches: Vec<TextMatch>,
    key: Option<SearchKey>,
    built_at: Option<Instant>,
    current: Option<usize>,
    scroll_to_current: bool,
    focus: bool,
    was_open: bool,
    message: Option<(Severity, String)>,
}

impl Default for FindReplace {
    fn default() -> Self {
        Self {
            find: String::new(),
            replace: String::new(),
            whole_word: false,
            case_sensitive: false,
            key_names: false,
            selection_only: false,
            visible_only: false,
            matches: Vec::new(),
            key: None,
            built_at: None,
            current: None,
            scroll_to_current: false,
            focus: true,
            was_open: false,
            message: None,
        }
    }
}

pub(crate) fn field_label(field: &TextField) -> String {
    match field {
        TextField::Value(k) => k.clone(),
        TextField::KeyName(k) => tf!("entities.fr_key_name", k),
        TextField::Connection { index, part } => {
            let part = match part {
                ConnPart::Output => t!("entities.col_output"),
                ConnPart::Target => t!("entities.col_target"),
                ConnPart::Input => t!("entities.col_input"),
                ConnPart::Param => t!("entities.col_param"),
            };
            tf!("entities.fr_output_field", index + 1, part)
        }
    }
}

impl FindReplace {
    pub fn set_find(&mut self, text: &str, replace: &str) {
        text.clone_into(&mut self.find);
        replace.clone_into(&mut self.replace);
        self.key = None;
    }

    #[cfg(test)]
    pub fn matches(&self) -> &[TextMatch] {
        &self.matches
    }

    fn query(&self, state: &AppState) -> TextQuery {
        let only = self.selection_only.then(|| SelectionSummary::of(&state.doc).entities);
        TextQuery {
            find: self.find.clone(),
            whole_word: self.whole_word,
            case_sensitive: self.case_sensitive,
            key_names: self.key_names,
            only,
            visible_only: self.visible_only,
        }
    }

    fn refresh(&mut self, ctx: &egui::Context, state: &AppState) {
        let selection = if self.selection_only { SelectionSummary::of(&state.doc).entities } else { Vec::new() };
        let key: SearchKey = (
            DocStamp::of(&state.doc),
            self.find.clone(),
            self.whole_word,
            self.case_sensitive,
            self.key_names,
            self.selection_only,
            self.visible_only,
            selection,
        );
        if self.key.as_ref() == Some(&key) {
            return;
        }
        let only_doc = self.key.as_ref().is_some_and(|k| k.1 == key.1 && k.2 == key.2 && k.3 == key.3 && k.4 == key.4);
        if only_doc && crate::dialogs::throttled(ctx, self.built_at, state.doc.scene().entities().len()) {
            return;
        }
        self.rebuild(state);
        self.key = Some(key);
    }

    fn rebuild(&mut self, state: &AppState) {
        self.matches = state.doc.find_text(&self.query(state));
        self.built_at = Some(Instant::now());
        self.current = self.current.filter(|c| *c < self.matches.len());
    }

    pub fn find_next(&mut self, state: &mut AppState) {
        if self.matches.is_empty() {
            self.current = None;
            return;
        }
        let next = self.current.map_or(0, |c| (c + 1) % self.matches.len());
        self.current = Some(next);
        self.scroll_to_current = true;
        self.show_match(state, next);
    }

    fn show_match(&self, state: &mut AppState, row: usize) {
        let Some(m) = self.matches.get(row) else { return };
        if m.world {
            return;
        }
        state.doc.set_selection(&[hp_editor::ObjRef::Entity(m.entity)]);
        state.request(hp_app::Action::Center2dOnSelection);
        state.request(hp_app::Action::Center3dOnSelection);
    }

    pub fn replace_current(&mut self, state: &mut AppState) -> usize {
        let Some(m) = self.current.and_then(|c| self.matches.get(c)).cloned() else {
            self.find_next(state);
            return 0;
        };
        let q = self.query(state);
        let n = self.run(state, &q, &[m]);
        self.key = None;
        self.rebuild(state);
        if self.matches.is_empty() {
            self.current = None;
        } else {
            let row = self.current.unwrap_or(0).min(self.matches.len() - 1);
            self.current = Some(row);
            self.scroll_to_current = true;
            self.show_match(state, row);
        }
        n
    }

    pub fn replace_all(&mut self, state: &mut AppState) -> usize {
        let q = self.query(state);
        let matches = self.matches.clone();
        let n = self.run(state, &q, &matches);
        self.current = None;
        self.key = None;
        n
    }

    fn run(&mut self, state: &mut AppState, q: &TextQuery, matches: &[TextMatch]) -> usize {
        if self.replace.contains('"') {
            let text = t!("entities.no_quotes").to_owned();
            state.notify(Severity::Warning, text.clone());
            self.message = Some((Severity::Warning, text));
            return 0;
        }
        match state.doc.replace_text(q, matches, &self.replace) {
            Ok(n) => {
                let text = tf!("entities.fr_replaced", n);
                let severity = if n > 0 { Severity::Success } else { Severity::Info };
                state.notify(severity, text.clone());
                self.message = Some((severity, text));
                n
            }
            Err(e) => {
                let text = tf!("entities.edit_failed", e);
                state.notify(Severity::Error, text.clone());
                self.message = Some((Severity::Error, text));
                0
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState, text_focus_before: bool) {
        if !state.runtime.entities.show_find_replace {
            self.was_open = false;
            return;
        }
        if !self.was_open {
            self.was_open = true;
            self.focus = true;
            self.message = None;
        }
        let mut open = true;
        let resp = hp_app::chrome::Window::new(t!("entities.fr_title"))
            .id(Id::unique("hp_ui_entities.find_replace_window"))
            .open(&mut open)
            .default_size([520.0, 440.0])
            .min_width(380.0)
            .resizable(true)
            .collapsible(false)
            .show(ctx, |ui| self.ui(ui, state));
        if crate::dialogs::escape_closes(ctx, resp.as_ref().map(|r| r.response.layer_id), text_focus_before) {
            open = false;
        }
        state.runtime.entities.show_find_replace = open;
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let mut enter_find = false;
        egui::Grid::new("hp_ui_entities.fr_fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label(t!("entities.fr_find"));
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.find)
                    .hint_text(t!("entities.fr_find_hint"))
                    .desired_width(f32::INFINITY),
            );
            if std::mem::take(&mut self.focus) {
                r.request_focus();
            }
            enter_find |= r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            ui.end_row();
            ui.label(t!("entities.fr_replace"));
            ui.add(egui::TextEdit::singleline(&mut self.replace).desired_width(f32::INFINITY));
            ui.end_row();
        });
        hp_app::widgets::choice_row_wrapped(ui, |ui| {
            ui.checkbox(&mut self.whole_word, t!("entities.fr_whole_word"));
            ui.checkbox(&mut self.case_sensitive, t!("entities.fr_case"));
            ui.checkbox(&mut self.key_names, t!("entities.fr_key_names"))
                .on_hover_text(t!("entities.fr_key_names_tip"));
        });
        hp_app::widgets::choice_row_wrapped(ui, |ui| {
            ui.radio_value(&mut self.selection_only, false, t!("entities.fr_whole_map"));
            ui.radio_value(&mut self.selection_only, true, t!("entities.fr_selection"));
            ui.checkbox(&mut self.visible_only, t!("entities.visible_only"));
        });
        self.refresh(ui.ctx(), state);
        if enter_find {
            self.find_next(state);
            self.focus = true;
        }

        let n = self.matches.len();
        let occurrences: usize = self.matches.iter().map(|m| m.ranges.len()).sum();
        let mut entities: Vec<(EntityId, bool)> = self.matches.iter().map(|m| (m.entity, m.world)).collect();
        entities.dedup();
        ui.horizontal(|ui| {
            let text = if self.find.is_empty() {
                t!("entities.find_empty").to_owned()
            } else {
                tf!("entities.fr_count", occurrences, n, entities.len())
            };
            ui.label(RichText::new(text).weak());
        });
        let has_find = !self.find.is_empty();
        let quote = self.replace.contains('"');
        if quote {
            ui.colored_label(theme::warning(), t!("entities.no_quotes"));
        }
        ui.horizontal(|ui| {
            let next = ui.add_enabled(n > 0, egui::Button::new(t!("entities.fr_find_next")));
            if next.on_hover_text(hp_app::actions::key_label(Key::Enter)).clicked() {
                self.find_next(state);
            }
            if ui.add_enabled(n > 0 && !quote, egui::Button::new(t!("entities.fr_replace_one"))).clicked() {
                self.replace_current(state);
            }
            let all = ui.add_enabled(n > 0 && has_find && !quote, egui::Button::new(t!("entities.fr_replace_all")));
            if all.clicked() {
                self.replace_all(state);
            }
            if ui.button(t!("common.close")).clicked() {
                state.runtime.entities.show_find_replace = false;
            }
        });
        if let Some((sev, text)) = &self.message {
            ui.colored_label(sev.color(), text);
        }
        ui.separator();
        self.results_ui(ui, state);
    }

    fn results_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let n = self.matches.len();
        let widths = column_widths(ui.available_width(), &[1.1, 0.9, 1.6], 50.0);
        let headers = [t!("entities.fr_col_entity"), t!("entities.fr_col_field"), t!("entities.fr_col_text")];
        table_header(ui, &headers, &widths, None);
        if n == 0 {
            empty_note(ui, if self.find.is_empty() { t!("entities.find_empty") } else { t!("entities.no_match") });
            return;
        }
        let mut area = ScrollArea::vertical().id_salt("hp_ui_entities.fr_rows").auto_shrink([false, false]);
        if std::mem::take(&mut self.scroll_to_current)
            && let Some(c) = self.current
        {
            let pitch = ROW_HEIGHT + ui.spacing().item_spacing.y;
            area = area.vertical_scroll_offset((c as f32 * pitch - 60.0).max(0.0));
        }
        let mut clicked = None;
        let mut activate = None;
        area.show_rows(ui, ROW_HEIGHT, n, |ui, range| {
            let scene = state.doc.scene();
            for i in range {
                let Some(m) = self.matches.get(i) else { continue };
                let who = if m.world {
                    "worldspawn".to_owned()
                } else {
                    match scene.entity(m.entity).map(|n| &n.entity) {
                        Some(e) => match e.get("targetname").filter(|n| !n.is_empty()) {
                            Some(name) => format!("{} ({name})", e.classname),
                            None => e.classname.clone(),
                        },
                        None => String::new(),
                    }
                };
                let field = field_label(&m.field);
                let cells = [(who.as_str(), None), (field.as_str(), None), (m.text.as_str(), None)];
                let resp = table_row(ui, &cells, &widths, self.current == Some(i), i % 2 == 1);
                let resp = resp.on_hover_text(&m.text);
                if resp.clicked() {
                    clicked = Some(i);
                }
                if resp.double_clicked() {
                    activate = Some(i);
                }
            }
        });
        if let Some(i) = clicked {
            self.current = Some(i);
        }
        if let Some(i) = activate {
            self.current = Some(i);
            self.show_match(state, i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_next_replace_and_replace_all() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut fr = FindReplace::default();
        fr.set_find("main_light", "lamp");
        fr.refresh(&egui::Context::default(), &state);
        assert_eq!(fr.matches().len(), 2);
        fr.find_next(&mut state);
        assert_eq!(fr.current, Some(0));
        fr.find_next(&mut state);
        assert_eq!(fr.current, Some(1));
        assert!(state.doc.selection().objects.iter().eq([hp_editor::ObjRef::Entity(6)].iter()));
        fr.find_next(&mut state);
        assert_eq!(fr.current, Some(0), "wraps around");
        assert_eq!(fr.replace_current(&mut state), 1);
        assert_eq!(fr.matches().len(), 1);
        assert_eq!(fr.replace_all(&mut state), 1);
        assert_eq!(state.doc.history_len().0, 2);
        assert_eq!(state.doc.scene().entity(5).unwrap().entity.get("targetname"), Some("lamp"));
        assert_eq!(state.doc.scene().entity(6).unwrap().entity.connections[0].target, "lamp");
    }

    #[test]
    fn quotes_are_refused_and_scope_is_respected() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let mut fr = FindReplace::default();
        fr.set_find("1", "a\"b");
        fr.refresh(&egui::Context::default(), &state);
        assert!(!fr.matches().is_empty());
        assert_eq!(fr.replace_all(&mut state), 0);
        assert_eq!(state.doc.history_len().0, 0);
        fr.selection_only = true;
        state.doc.set_selection(&[hp_editor::ObjRef::Entity(6)]);
        fr.refresh(&egui::Context::default(), &state);
        assert!(fr.matches().iter().all(|m| m.entity == 6 && !m.world));
    }

    #[test]
    fn enter_in_the_find_field_goes_to_the_next_match() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.runtime.entities.show_find_replace = true;
        let mut fr = FindReplace::default();
        let ctx = egui::Context::default();
        let enter = egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let frames =
            [vec![], vec![egui::Event::Text("main_light".into())], vec![enter.clone()], vec![], vec![enter], vec![]];
        let mut seen = Vec::new();
        for (i, events) in frames.into_iter().enumerate() {
            let input = egui::RawInput { events, time: Some(i as f64 * 0.1), focused: true, ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                let ctx = ui.ctx().clone();
                fr.show(&ctx, &mut state, false);
            });
            out.textures_delta.clear();
            seen.push(fr.current);
        }
        assert_eq!(fr.find, "main_light");
        assert_eq!(seen[2], Some(0), "{seen:?}");
        assert_eq!(seen[4], Some(1), "{seen:?}");
    }

    #[test]
    fn window_draws() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.runtime.entities.show_find_replace = true;
        let mut fr = FindReplace::default();
        fr.set_find("light", "");
        hp_app::testing::run_ctx(&mut state, |ctx, state| fr.show(ctx, state, false));
        assert!(state.runtime.entities.show_find_replace);
        assert!(!fr.matches().is_empty());
        assert!(!field_label(&TextField::Connection { index: 0, part: ConnPart::Param }).is_empty());
    }
}
