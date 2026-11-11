use std::path::PathBuf;

use egui::{Color32, Key, Modifiers, RichText};
use hp_app::actions::key_label;
use hp_app::i18n::{Lang, tr_in};
use hp_app::{Action, AppState, t, tf};

use crate::common::{accent_bg, accent_fg, accent_text, action_enabled, action_icon, category_label, modal_layer};
use crate::fuzzy::{Query, fold, rank};
use crate::keys::shortcut_text;

const MAX_RESULTS: usize = 200;
const MRU_LEN: usize = 6;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Action(Action),
    Recent(PathBuf),
    Language(Lang),
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub kind: EntryKind,
    pub label: String,
    pub detail: String,
    pub shortcut: String,
    pub icon: &'static str,
    hay: [String; 3],
}

pub fn build_entries(state: &AppState) -> Vec<Entry> {
    let mut out = Vec::with_capacity(Action::ALL.len() + state.settings.recent_files.len() + 2);
    for &action in Action::ALL {
        let key = action.label_key();
        let label = action.label().to_owned();
        let english = tr_in(&key, Lang::En);
        let detail = match action.tool() {
            Some(_) => t!("shell.palette.tool").to_owned(),
            None => crate::menu::menu_path_text(action, None)
                .unwrap_or_else(|| category_label(action.category()).to_owned()),
        };
        out.push(Entry {
            hay: [fold(&label), fold(english), fold(&action.id().replace('_', " "))],
            kind: EntryKind::Action(action),
            label,
            detail,
            shortcut: shortcut_text(&state.keys, action),
            icon: action_icon(action).unwrap_or("▪"),
        });
    }
    for path in &state.settings.recent_files {
        let name =
            path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
        let label = tf!("shell.palette.open_recent_named", name);
        out.push(Entry {
            hay: [
                fold(&label),
                fold(&format!("{} {name}", tr_in("shell.palette.open_recent", Lang::En))),
                fold(&path.display().to_string()),
            ],
            kind: EntryKind::Recent(path.clone()),
            label,
            detail: t!("menu.recent").to_owned(),
            shortcut: String::new(),
            icon: "🗁",
        });
    }
    for l in Lang::ALL {
        let label = tf!("shell.palette.language_named", l.native_name());
        out.push(Entry {
            hay: [fold(&label), fold(&format!("language {}", l.native_name())), fold(&format!("lang {}", l.code()))],
            kind: EntryKind::Language(l),
            label,
            detail: t!("menu.view").to_owned(),
            shortcut: String::new(),
            icon: "🌐",
        });
    }
    out
}

pub fn search(entries: &[Entry], query: &str, mru: &[Action]) -> Vec<usize> {
    let q = Query::new(query);
    if q.is_empty() {
        let mut out: Vec<usize> =
            mru.iter().filter_map(|a| entries.iter().position(|e| e.kind == EntryKind::Action(*a))).collect();
        let rest: Vec<usize> = (0..entries.len()).filter(|i| !out.contains(i)).collect();
        out.extend(rest);
        out.truncate(MAX_RESULTS);
        return out;
    }
    let mut ranked = rank(&q, entries, |e| e.hay.iter().map(String::as_str).collect());
    for (i, score) in &mut ranked {
        if let EntryKind::Action(a) = entries[*i].kind
            && let Some(pos) = mru.iter().position(|m| *m == a)
        {
            *score += 6 - pos as i32;
        }
    }
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked.into_iter().take(MAX_RESULTS).map(|(i, _)| i).collect()
}

#[derive(Debug, Default)]
pub struct CommandPalette {
    pub query: String,
    entries: Vec<Entry>,
    results: Vec<usize>,
    selected: usize,
    results_for: Option<String>,
    was_open: bool,
    scroll_to_selected: bool,
    mru: Vec<Action>,
}

impl CommandPalette {
    pub fn result_labels(&self) -> Vec<&str> {
        self.results.iter().filter_map(|&i| self.entries.get(i)).map(|e| e.label.as_str()).collect()
    }

    pub fn recent_actions(&self) -> &[Action] {
        &self.mru
    }

    fn open(&mut self, state: &AppState) {
        self.entries = build_entries(state);
        self.query.clear();
        self.results_for = None;
        self.selected = 0;
        self.scroll_to_selected = true;
    }

    fn update_results(&mut self) {
        if self.results_for.as_deref() == Some(self.query.as_str()) {
            return;
        }
        self.results = search(&self.entries, &self.query, &self.mru);
        self.results_for = Some(self.query.clone());
        self.selected = 0;
        self.scroll_to_selected = true;
    }

    fn move_selection(&mut self, delta: i64) {
        if self.results.is_empty() {
            return;
        }
        let n = self.results.len() as i64;
        self.selected = (self.selected as i64 + delta).clamp(0, n - 1) as usize;
        self.scroll_to_selected = true;
    }

    fn run(&mut self, state: &mut AppState, result: usize) -> bool {
        let Some(entry) = self.results.get(result).and_then(|&i| self.entries.get(i)) else {
            return false;
        };
        match entry.kind.clone() {
            EntryKind::Action(action) => {
                if !action_enabled(state, action) {
                    return false;
                }
                if action != Action::CommandPalette {
                    state.request(action);
                }
                self.mru.retain(|a| *a != action);
                self.mru.insert(0, action);
                self.mru.truncate(MRU_LEN);
            }
            EntryKind::Recent(path) => state.requested_open = Some(path),
            EntryKind::Language(l) => crate::menu::switch_language(state, l),
        }
        true
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let open = state.ui.show_command_palette;
        if open && !self.was_open {
            self.open(state);
        }
        self.was_open = open;
        if !open {
            return;
        }
        self.update_results();

        let id = egui::Id::unique("hp_shell_command_palette");
        let on_top = ctx.memory(|m| m.top_modal_layer()).is_none_or(|l| l == modal_layer(id));
        let keys = on_top.then(|| {
            ctx.input_mut(|i| {
                (
                    i.consume_key(Modifiers::NONE, Key::ArrowUp),
                    i.consume_key(Modifiers::NONE, Key::ArrowDown),
                    i.consume_key(Modifiers::NONE, Key::PageUp),
                    i.consume_key(Modifiers::NONE, Key::PageDown),
                    i.consume_key(Modifiers::NONE, Key::Enter),
                    i.consume_key(Modifiers::NONE, Key::Escape),
                )
            })
        });
        let (up, down, page_up, page_down, enter, escape) = keys.unwrap_or_default();
        if up {
            self.move_selection(-1);
        }
        if down {
            self.move_selection(1);
        }
        if page_up {
            self.move_selection(-10);
        }
        if page_down {
            self.move_selection(10);
        }
        let mut close = escape;
        if enter && self.run(state, self.selected) {
            close = true;
        }

        let area = egui::Modal::default_area(id).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 70.0));
        let mut clicked = None;
        let modal = egui::Modal::new(id).area(area).backdrop_color(Color32::from_black_alpha(60)).show(ctx, |ui| {
            ui.set_width(580.0);
            let edit = egui::TextEdit::singleline(&mut self.query)
                .hint_text(t!("shell.palette.hint"))
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Heading);
            let r = ui.add(edit);
            if !r.has_focus() {
                r.request_focus();
            }
            if r.changed() {
                self.update_results();
            }
            ui.add_space(4.0);
            ui.separator();
            if self.results.is_empty() {
                ui.add_space(6.0);
                ui.label(RichText::new(t!("shell.palette.no_results")).color(ui.visuals().weak_text_color()));
                ui.add_space(6.0);
            } else {
                egui::ScrollArea::vertical().max_height(400.0).auto_shrink([false, true]).show(ui, |ui| {
                    for (row, &ei) in self.results.iter().enumerate() {
                        let Some(entry) = self.entries.get(ei) else { continue };
                        let enabled = match entry.kind {
                            EntryKind::Action(a) => action_enabled(state, a),
                            _ => true,
                        };
                        let r = palette_row(ui, entry, row == self.selected, enabled);
                        if row == self.selected && self.scroll_to_selected {
                            r.scroll_to_me(None);
                        }
                        if r.clicked() {
                            clicked = Some(row);
                        }
                        if r.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO) {
                            self.selected = row;
                        }
                    }
                });
                self.scroll_to_selected = false;
            }
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                let weak = ui.visuals().weak_text_color();
                let footer = tf!("shell.palette.footer", "⏶⏷", key_label(Key::Enter), key_label(Key::Escape));
                ui.label(RichText::new(footer).small().color(weak));
            });
        });
        if let Some(row) = clicked
            && self.run(state, row)
        {
            close = true;
        }
        if modal.backdrop_response.clicked() {
            close = true;
        }
        if close {
            state.ui.show_command_palette = false;
            self.was_open = false;
        }
    }
}

fn palette_row(ui: &mut egui::Ui, entry: &Entry, selected: bool, enabled: bool) -> egui::Response {
    let height = 24.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::CLICK);
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let painter = ui.painter();
        let selected_fg = accent_fg(visuals);
        if selected {
            painter.rect_filled(rect, 4.0, accent_bg(visuals));
        } else if response.hovered() {
            painter.rect_filled(rect, 4.0, visuals.widgets.hovered.weak_bg_fill);
        }
        let text_color = if !enabled {
            visuals.weak_text_color().gamma_multiply(0.6)
        } else if selected {
            selected_fg
        } else {
            visuals.text_color()
        };
        let weak = visuals.weak_text_color();
        let body = egui::TextStyle::Body.resolve(ui.style());
        let small = egui::TextStyle::Small.resolve(ui.style());
        let mono = egui::TextStyle::Monospace.resolve(ui.style());
        let y = rect.center().y;
        painter.text(
            egui::pos2(rect.left() + 16.0, y),
            egui::Align2::CENTER_CENTER,
            entry.icon,
            body.clone(),
            if selected { accent_text(visuals) } else { weak },
        );
        let label_rect =
            painter.text(egui::pos2(rect.left() + 34.0, y), egui::Align2::LEFT_CENTER, &entry.label, body, text_color);
        let mut right = rect.right() - 8.0;
        if !entry.shortcut.is_empty() {
            let r = painter.text(
                egui::pos2(right, y),
                egui::Align2::RIGHT_CENTER,
                &entry.shortcut,
                mono,
                if selected { selected_fg } else { weak },
            );
            right = r.left() - 12.0;
        }
        if right > label_rect.right() + 40.0 {
            painter.with_clip_rect(egui::Rect::from_x_y_ranges(label_rect.right() + 8.0..=right, rect.y_range())).text(
                egui::pos2(right, y),
                egui::Align2::RIGHT_CENTER,
                &entry.detail,
                small,
                weak,
            );
        }
    }
    response
}

pub(crate) const TR: hp_app::i18n::TrTable = &[
    ("shell.palette.hint", "Type a command, tool or map...", "Tapez une commande, un outil ou une map..."),
    ("shell.palette.no_results", "Nothing found", "Aucun résultat"),
    ("shell.palette.footer", "{0} navigate   {1} run   {2} close", "{0} naviguer   {1} exécuter   {2} fermer"),
    ("shell.palette.tool", "Tool", "Outil"),
    ("shell.palette.open_recent", "Open", "Ouvrir"),
    ("shell.palette.open_recent_named", "Open: {0}", "Ouvrir : {0}"),
    ("shell.palette.language_named", "Language: {0}", "Langue : {0}"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(entries: &[Entry], idx: &[usize]) -> Vec<String> {
        idx.iter().map(|&i| entries[i].label.clone()).collect()
    }

    #[test]
    fn finds_actions_in_both_languages() {
        let _g = crate::tests::lang_guard();
        let mut state = hp_app::testing::test_state();
        crate::register_translations();
        state.settings.recent_files.push(PathBuf::from("C:/maps/gm_construct_test.vmf"));
        hp_app::i18n::set_lang(Lang::Fr);
        let entries = build_entries(&state);
        let r = search(&entries, "enregistrer sous", &[]);
        assert_eq!(entries[r[0]].kind, EntryKind::Action(Action::SaveMapAs), "{:?}", labels(&entries, &r));
        let r = search(&entries, "save as", &[]);
        assert_eq!(entries[r[0]].kind, EntryKind::Action(Action::SaveMapAs), "{:?}", labels(&entries, &r));
        let r = search(&entries, "effacer la selection", &[]);
        assert_eq!(entries[r[0]].kind, EntryKind::Action(Action::SelectNone), "{:?}", labels(&entries, &r));
        let r = search(&entries, "options", &[]);
        assert_eq!(entries[r[0]].kind, EntryKind::Action(Action::Settings), "{:?}", labels(&entries, &r));
        let r = search(&entries, "settings", &[]);
        assert!(
            r.iter().any(|&i| entries[i].kind == EntryKind::Action(Action::Settings)),
            "{:?}",
            labels(&entries, &r)
        );
        let r = search(&entries, "tool_clip", &[]);
        assert_eq!(entries[r[0]].kind, EntryKind::Action(Action::ToolClip), "{:?}", labels(&entries, &r));
        let r = search(&entries, "construct", &[]);
        assert!(matches!(entries[r[0]].kind, EntryKind::Recent(_)), "{:?}", labels(&entries, &r));
        hp_app::i18n::set_lang(Lang::En);
    }

    #[test]
    fn empty_query_lists_recent_first() {
        let state = hp_app::testing::test_state();
        let entries = build_entries(&state);
        let r = search(&entries, "", &[Action::CheckProblems, Action::Undo]);
        assert_eq!(entries[r[0]].kind, EntryKind::Action(Action::CheckProblems));
        assert_eq!(entries[r[1]].kind, EntryKind::Action(Action::Undo));
        assert_eq!(r.len(), entries.len().min(MAX_RESULTS));
    }

    #[test]
    fn running_entries() {
        let mut state = hp_app::testing::test_state();
        let mut p = CommandPalette::default();
        p.open(&state);
        p.query = "select all".into();
        p.update_results();
        assert!(p.run(&mut state, 0));
        assert!(state.requested_actions.contains(&Action::SelectAll), "{:?}", state.requested_actions);
        assert_eq!(p.recent_actions(), [Action::SelectAll]);
        p.query = "undo".into();
        p.update_results();
        let undo_row = p.results.iter().position(|&i| p.entries[i].kind == EntryKind::Action(Action::Undo));
        assert!(undo_row.is_some());
        assert!(!p.run(&mut state, undo_row.unwrap_or(0)));
        p.move_selection(-5);
        assert_eq!(p.selected, 0);
        p.move_selection(10_000);
        assert_eq!(p.selected, p.results.len() - 1);
    }
}
