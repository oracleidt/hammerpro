use egui::{RichText, TextStyle, Ui};
use hp_app::console::{Console, ConsoleLine};
use hp_app::{AppState, Severity, t, tf};

use crate::common::{fmt_clock, severity_color};
use crate::fuzzy::{fold, fold_into};

pub const SEVERITIES: [Severity; 4] = [Severity::Info, Severity::Success, Severity::Warning, Severity::Error];

fn severity_index(s: Severity) -> usize {
    match s {
        Severity::Info => 0,
        Severity::Success => 1,
        Severity::Warning => 2,
        Severity::Error => 3,
    }
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Info => t!("common.info"),
        Severity::Success => t!("shell.console.success"),
        Severity::Warning => t!("common.warning"),
        Severity::Error => t!("common.error"),
    }
}

pub fn contains_ci(hay: &str, needle_lower: &str) -> bool {
    let n = needle_lower.as_bytes();
    if n.is_empty() {
        return true;
    }
    let h = hay.as_bytes();
    if n.len() > h.len() {
        return false;
    }
    h.windows(n.len()).any(|w| w.iter().zip(n).all(|(a, b)| a.to_ascii_lowercase() == *b))
}

pub fn contains_folded(hay: &str, needle_folded: &str, buf: &mut String) -> bool {
    if needle_folded.is_empty() {
        return true;
    }
    if hay.is_ascii() {
        return contains_ci(hay, needle_folded);
    }
    fold_into(hay, buf);
    buf.contains(needle_folded)
}

pub const MAX_EXPANDED_LINES: usize = 400;

pub fn line_count(text: &str) -> usize {
    text.lines().count().max(1)
}

pub fn first_line(text: &str) -> &str {
    text.lines().next().unwrap_or("")
}

pub fn row_starts(spans: impl Iterator<Item = usize>) -> Vec<usize> {
    let mut starts = vec![0];
    let mut at = 0;
    for span in spans {
        at += span.max(1);
        starts.push(at);
    }
    starts
}

pub fn rows_in(starts: &[usize], row_height: f32, top: f32, bottom: f32) -> std::ops::Range<usize> {
    let rows = starts.len().saturating_sub(1);
    if rows == 0 || row_height.is_nan() || row_height <= 0.0 {
        return 0..0;
    }
    let first_unit = (top / row_height).floor().max(0.0) as usize;
    let last_unit = (bottom / row_height).ceil().max(0.0) as usize;
    let first = starts[1..].partition_point(|end| *end <= first_unit).min(rows);
    let last = starts[..rows].partition_point(|start| *start < last_unit).max(first);
    first..last
}

type StartsKey = (u64, [bool; 4], String, u64);

#[derive(Debug)]
pub struct ConsoleView {
    pub show: [bool; 4],
    pub search: String,
    filtered: Vec<usize>,
    counts: [usize; 4],
    cache_key: Option<(u64, [bool; 4], String)>,
    labels: Option<HeaderLabels>,
    expanded: std::collections::HashSet<u64>,
    starts: Vec<usize>,
    starts_key: Option<StartsKey>,
    expanded_serial: u64,
}

fn line_id(line: &ConsoleLine) -> u64 {
    line.time.to_bits()
}

type HeaderLabels = ([usize; 4], usize, hp_app::i18n::Lang, [String; 4], String);

impl Default for ConsoleView {
    fn default() -> Self {
        Self {
            show: [true; 4],
            search: String::new(),
            filtered: Vec::new(),
            counts: [0; 4],
            cache_key: None,
            labels: None,
            expanded: std::collections::HashSet::new(),
            starts: vec![0],
            starts_key: None,
            expanded_serial: 0,
        }
    }
}

impl ConsoleView {
    pub fn refresh(&mut self, console: &Console) {
        if let Some((rev, show, search)) = &self.cache_key
            && *rev == console.revision
            && *show == self.show
            && *search == self.search
        {
            return;
        }
        let needle = fold(self.search.trim());
        let mut buf = String::new();
        self.filtered.clear();
        self.counts = [0; 4];
        for (i, line) in console.lines().enumerate() {
            let s = severity_index(line.severity);
            self.counts[s] += 1;
            if self.show[s] && contains_folded(&line.text, &needle, &mut buf) {
                self.filtered.push(i);
            }
        }
        self.cache_key = Some((console.revision, self.show, self.search.clone()));
    }

    pub fn visible_rows(&self) -> usize {
        self.filtered.len()
    }

    pub fn is_expanded(&self, line: &ConsoleLine) -> bool {
        self.expanded.contains(&line_id(line))
    }

    pub fn toggle(&mut self, line: &ConsoleLine) {
        let id = line_id(line);
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        self.expanded_serial = self.expanded_serial.wrapping_add(1);
    }

    pub fn row_span(&self, line: &ConsoleLine) -> usize {
        if self.is_expanded(line) { line_count(&line.text).min(MAX_EXPANDED_LINES) } else { 1 }
    }

    pub fn layout(&mut self, console: &Console) -> &[usize] {
        self.refresh(console);
        let key = self.cache_key.clone().map(|(rev, show, search)| (rev, show, search, self.expanded_serial));
        if self.starts_key != key {
            if !self.expanded.is_empty() {
                let alive: std::collections::HashSet<u64> = console.lines().map(line_id).collect();
                self.expanded.retain(|id| alive.contains(id));
            }
            let lines: Vec<&ConsoleLine> = console.lines().collect();
            let starts = row_starts(self.filtered.iter().map(|&i| lines.get(i).map_or(1, |line| self.row_span(line))));
            self.starts = starts;
            self.starts_key = key;
        }
        &self.starts
    }

    pub fn counts(&self) -> [usize; 4] {
        self.counts
    }

    pub fn visible_text(&self, console: &Console) -> String {
        let mut out = String::new();
        for &i in &self.filtered {
            if let Some(line) = console.lines().nth(i) {
                out.push_str(&format_line(line));
                out.push('\n');
            }
        }
        out
    }

    pub fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.refresh(&state.console);
        self.header(ui, state);
        ui.add_space(2.0);
        let row_height = ui.text_style_height(&TextStyle::Monospace).max(ui.text_style_height(&TextStyle::Small));
        let total = self.filtered.len();
        if total == 0 {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                let text =
                    if state.console.is_empty() { t!("shell.console.empty") } else { t!("shell.console.no_match") };
                ui.label(RichText::new(text).color(ui.visuals().weak_text_color()));
            });
            return;
        }
        let mut copy_line = None;
        let mut toggled = None;
        self.layout(&state.console);
        let units = self.starts.last().copied().unwrap_or(0);
        egui::ScrollArea::vertical()
            .id_salt("hp_shell_console_rows")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show_viewport(ui, |ui, viewport| {
                ui.set_height(row_height * units as f32);
                let range = rows_in(&self.starts, row_height, viewport.min.y, viewport.max.y);
                let Some(&first) = self.starts.get(range.start) else { return };
                let top = ui.max_rect().top() + first as f32 * row_height;
                let rect = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.max_rect().bottom());
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.skip_ahead_auto_ids(range.start);
                    for row in range {
                        let Some(line) = self.filtered.get(row).and_then(|&i| state.console.lines().nth(i)) else {
                            continue;
                        };
                        let span = self.starts[row + 1] - self.starts[row];
                        let r = line_row(ui, line, row_height, span, self.is_expanded(line));
                        if let Some(text) = r.copy {
                            copy_line = Some(text);
                        }
                        if r.toggle {
                            toggled = Some(line.clone());
                        }
                    }
                });
            });
        if let Some(line) = toggled {
            self.toggle(&line);
        }
        if let Some(text) = copy_line {
            ui.ctx().copy_text(text);
        }
    }

    fn update_labels(&mut self) {
        let lang = hp_app::i18n::lang();
        let lines = self.filtered.len();
        if self.labels.as_ref().is_some_and(|(c, l, g, _, _)| *c == self.counts && *l == lines && *g == lang) {
            return;
        }
        let counts = self.counts;
        let chips = std::array::from_fn(|i| format!("{} {}", SEVERITIES[i].icon(), counts[i]));
        self.labels = Some((counts, lines, lang, chips, tf!("shell.console.lines", lines)));
    }

    fn header(&mut self, ui: &mut Ui, state: &mut AppState) {
        self.update_labels();
        let labels = self.labels.take();
        let (chips, lines) = labels.as_ref().map_or((None, ""), |(_, _, _, c, l)| (Some(c), l.as_str()));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for (i, sev) in SEVERITIES.into_iter().enumerate() {
                let label = chips.map_or("", |c| c[i].as_str());
                let text = RichText::new(label).color(if self.show[i] {
                    severity_color(ui.visuals(), sev)
                } else {
                    ui.visuals().weak_text_color()
                });
                let chip = ui.add(egui::Button::selectable(self.show[i], text).min_size(egui::vec2(44.0, 20.0)));
                if crate::common::lazy_hover_text(chip, || tf!("shell.console.filter_tip", severity_label(sev)))
                    .clicked()
                {
                    self.show[i] = !self.show[i];
                }
            }
            ui.separator();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("🗑").on_hover_text(t!("shell.console.clear")).clicked() {
                    state.console.clear();
                }
                if ui.small_button("🗐").on_hover_text(t!("shell.console.copy_all")).clicked() {
                    let text = self.visible_text(&state.console);
                    ui.ctx().copy_text(text);
                }
                ui.label(RichText::new(lines).small().color(ui.visuals().weak_text_color()));
                let width = ui.available_width().clamp(120.0, 320.0);
                ui.allocate_ui_with_layout(
                    egui::vec2(width, ui.available_height()),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        hp_app::widgets::search_box(ui, &mut self.search, t!("shell.console.search"));
                    },
                );
            });
        });
        self.labels = labels;
    }
}

pub fn format_line(line: &ConsoleLine) -> String {
    format!("[{}] {}", fmt_clock(line.time), line.text)
}

#[derive(Debug, Default)]
pub struct RowAction {
    pub copy: Option<String>,
    pub toggle: bool,
}

pub fn row_text(line: &ConsoleLine, expanded: bool) -> (String, Option<usize>) {
    let count = line_count(&line.text);
    if count <= 1 {
        return (line.text.clone(), None);
    }
    if expanded {
        let text: Vec<&str> = line.text.lines().take(MAX_EXPANDED_LINES).collect();
        (text.join("\n"), Some(count))
    } else {
        (first_line(&line.text).to_owned(), Some(count))
    }
}

fn line_row(ui: &mut Ui, line: &ConsoleLine, row_height: f32, span: usize, expanded: bool) -> RowAction {
    let height = row_height * span.max(1) as f32;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::CLICK);
    let (text, multi) = row_text(line, expanded);
    let mut text_clipped = false;
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let painter = ui.painter().with_clip_rect(hp_app::widgets::clip_x(rect, ui.clip_rect()));
        if response.hovered() {
            painter.rect_filled(rect, 2.0, visuals.widgets.hovered.weak_bg_fill.gamma_multiply(0.35));
        }
        let font = TextStyle::Monospace.resolve(ui.style());
        let small = TextStyle::Small.resolve(ui.style());
        let text_color = match line.severity {
            Severity::Info => visuals.text_color(),
            other => severity_color(visuals, other),
        };
        let y = rect.top() + row_height * 0.5;
        let mut x = rect.left() + 4.0;
        let r = painter.text(
            egui::pos2(x, y),
            egui::Align2::LEFT_CENTER,
            fmt_clock(line.time),
            small.clone(),
            visuals.weak_text_color(),
        );
        x = r.right() + 8.0;
        let r = painter.text(
            egui::pos2(x, y),
            egui::Align2::LEFT_CENTER,
            line.severity.icon(),
            font.clone(),
            severity_color(visuals, line.severity),
        );
        x = r.right() + 8.0;
        let galley = painter.layout_no_wrap(text, font, text_color);
        let first_row = galley.rows.first().map_or(row_height, |row| row.height());
        let right = x + galley.size().x;
        painter.galley(egui::pos2(x, y - first_row * 0.5), galley, text_color);
        text_clipped = right > rect.right();
        if let Some(count) = multi {
            let marker = if expanded {
                t!("shell.console.collapse").to_owned()
            } else {
                tf!("shell.console.more_lines", count - 1)
            };
            let inline = !expanded && right + 10.0 <= rect.right() - 6.0;
            let (at, align) = if inline {
                (right + 10.0, egui::Align2::LEFT_CENTER)
            } else {
                (rect.right() - 6.0, egui::Align2::RIGHT_CENTER)
            };
            let size = painter.layout_no_wrap(marker.clone(), small.clone(), visuals.weak_text_color()).size();
            let pill = align.anchor_size(egui::pos2(at, y), size).expand2(egui::vec2(4.0, 1.0));
            painter.rect_filled(pill, 3.0, visuals.extreme_bg_color);
            painter.text(egui::pos2(at, y), align, marker, small, visuals.weak_text_color());
        }
    }
    let response = match multi {
        Some(count) if !expanded => response.on_hover_text(tf!("shell.console.expand_tip", count)),
        Some(_) => response.on_hover_text(t!("shell.console.collapse_tip")),
        None if text_clipped => response.on_hover_text(&line.text),
        None => response,
    };
    let mut action = RowAction { copy: None, toggle: multi.is_some() && response.clicked() };
    response.context_menu(|ui| {
        if ui.button(t!("common.copy_text")).clicked() {
            action.copy = Some(line.text.clone());
        }
        crate::common::close_on_escape(ui);
    });
    action
}

pub(crate) const TR: hp_app::i18n::TrTable = &[
    ("shell.console.success", "Success", "Succès"),
    ("shell.console.filter_tip", "Show or hide messages: {0}", "Afficher ou masquer les messages : {0}"),
    ("shell.console.search", "Filter messages", "Filtrer les messages"),
    ("shell.console.clear", "Clear the console", "Vider la console"),
    ("shell.console.copy_all", "Copy visible messages", "Copier les messages visibles"),
    ("shell.console.lines", "Lines: {0}", "Lignes : {0}"),
    ("shell.console.empty", "No messages yet", "Aucun message pour l'instant"),
    ("shell.console.no_match", "No message matches the filters", "Aucun message ne correspond aux filtres"),
    ("shell.console.more_lines", "+{0} lines", "+{0} lignes"),
    ("shell.console.collapse", "Collapse", "Replier"),
    ("shell.console.expand_tip", "Click to show all {0} lines", "Cliquez pour afficher les {0} lignes"),
    (
        "shell.console.collapse_tip",
        "Click to show only the first line",
        "Cliquez pour n'afficher que la première ligne",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_insensitive_contains() {
        assert!(contains_ci("Loaded 42 Materials", "materials"));
        assert!(contains_ci("anything", ""));
        assert!(!contains_ci("short", "much longer needle"));
        assert!(contains_ci("Éclairage: OK", "ok"));
    }

    #[test]
    fn accent_insensitive_search() {
        let mut buf = String::new();
        let line = "Échec du chargement du contenu du jeu : dossier introuvable";
        assert!(contains_folded(line, &fold("échec"), &mut buf));
        assert!(contains_folded(line, &fold("ECHEC"), &mut buf));
        assert!(contains_folded(line, &fold("Échec du"), &mut buf));
        assert!(!contains_folded(line, &fold("réussite"), &mut buf));
        assert!(contains_folded("Mounted 12 VPKs", &fold("vpks"), &mut buf));
        assert!(contains_folded("Mounted", &fold("Mounté"), &mut buf));
        assert!(!contains_folded("Mounted", &fold("ж"), &mut buf));
        assert!(contains_folded("anything", "", &mut buf));
        let mut console = Console::default();
        console.error(line);
        console.info("Éclairage terminé");
        console.info("Lighting done");
        let mut view = ConsoleView { search: "echec".into(), ..Default::default() };
        view.refresh(&console);
        assert_eq!(view.visible_rows(), 1);
        view.search = "ECLAIRAGE TERMINE".into();
        view.refresh(&console);
        assert_eq!(view.visible_rows(), 1);
    }

    #[test]
    fn severity_colors_follow_the_theme() {
        use hp_app::settings::ThemeChoice;
        let dark = hp_app::theme::visuals(ThemeChoice::Dark);
        let light = hp_app::theme::visuals(ThemeChoice::Light);
        for sev in SEVERITIES {
            assert_eq!(severity_color(&dark, sev), sev.color());
            let c = severity_color(&light, sev);
            assert!(hp_app::theme::is_gray(c) && hp_app::theme::is_gray(severity_color(&dark, sev)));
            let luma = 0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32;
            assert!(luma < 140.0, "{sev:?} too pale on the light theme: {c:?}");
        }
    }

    fn text_rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, egui::Rect)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, egui::Rect)>) {
            match shape {
                egui::Shape::Text(t) => {
                    out.push((t.galley.text().to_owned(), egui::Rect::from_min_size(t.pos, t.galley.size())))
                }
                egui::Shape::Vec(list) => list.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for clipped in shapes {
            walk(&clipped.shape, &mut out);
        }
        out
    }

    fn drawn_rows(state: &mut AppState, view: &mut ConsoleView) -> Vec<(String, egui::Rect)> {
        let ctx = egui::Context::default();
        let mut shapes = Vec::new();
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 400.0))),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| view.ui(ui, state));
            });
            shapes = out.shapes;
        }
        text_rects(&shapes)
            .into_iter()
            .filter(|(text, _)| text.starts_with("line") || text.starts_with("stack") || text.contains("user32"))
            .collect()
    }

    #[test]
    fn a_stack_in_the_log_never_overlaps_the_other_rows() {
        let mut state = hp_app::testing::test_state();
        state.console.clear();
        state.console.info("line one");
        state
            .console
            .warn("stack of the main thread:\nhammerpro.exe+0x12\nuser32.dll CallWindowProcW\nDispatchMessageW");
        state.console.info("line three");
        let mut view = ConsoleView::default();
        let rows = drawn_rows(&mut state, &mut view);
        assert!(rows.iter().all(|(text, _)| !text.contains('\n')), "only first lines are drawn: {rows:?}");
        assert!(!rows.iter().any(|(text, _)| text.contains("user32")), "the stack stays folded");
        let mut sorted = rows.clone();
        sorted.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
        assert_eq!(sorted.len(), 3, "{sorted:?}");
        for pair in sorted.windows(2) {
            assert!(pair[0].1.bottom() <= pair[1].1.top() + 0.5, "rows overlap: {pair:?}");
        }
        let stack = state.console.lines().nth(1).unwrap().clone();
        view.toggle(&stack);
        let rows = drawn_rows(&mut state, &mut view);
        let mut sorted = rows.clone();
        sorted.sort_by(|a, b| a.1.top().total_cmp(&b.1.top()));
        let expanded = sorted.iter().find(|(text, _)| text.contains("user32")).expect("the expanded stack is drawn");
        let after = sorted.iter().find(|(text, _)| text == "line three").expect("the next row");
        assert!(expanded.1.bottom() <= after.1.top() + 0.5, "the expanded entry pushes the next row down: {sorted:?}");
        for pair in sorted.windows(2) {
            assert!(pair[0].1.bottom() <= pair[1].1.top() + 0.5, "rows overlap: {pair:?}");
        }
    }

    #[test]
    fn multi_line_entries_take_one_row_until_expanded() {
        let mut console = Console::default();
        console.info("Mounted 12 VPKs");
        console.warn("main thread stack:\nhammerpro.exe+0x12\nuser32.dll CallWindowProcW\nDispatchMessageW");
        console.error("Leak detected");
        let mut view = ConsoleView::default();
        assert_eq!(view.layout(&console), &[0, 1, 2, 3], "every entry is one row, the stack does not spill over");
        let stack = console.lines().nth(1).unwrap().clone();
        let (text, multi) = row_text(&stack, false);
        assert_eq!(text, "main thread stack:");
        assert_eq!(multi, Some(4));
        assert_eq!(row_text(console.lines().next().unwrap(), false), ("Mounted 12 VPKs".to_owned(), None));
        view.toggle(&stack);
        assert!(view.is_expanded(&stack));
        assert_eq!(view.layout(&console), &[0, 1, 5, 6], "the expanded entry pushes the next rows down");
        assert_eq!(row_text(&stack, true).0.lines().count(), 4);
        assert_eq!(rows_in(view.layout(&console), 10.0, 0.0, 10.0), 0..1);
        assert_eq!(rows_in(view.layout(&console), 10.0, 15.0, 25.0), 1..2, "inside the expanded entry");
        assert_eq!(rows_in(view.layout(&console), 10.0, 45.0, 70.0), 1..3);
        assert_eq!(rows_in(view.layout(&console), 10.0, 55.0, 60.0), 2..3);
        assert_eq!(rows_in(view.layout(&console), 10.0, 100.0, 200.0), 3..3);
        view.toggle(&stack);
        assert_eq!(view.layout(&console), &[0, 1, 2, 3]);
        assert!(view.visible_text(&console).contains("user32.dll CallWindowProcW"), "copying keeps the full text");
        console.clear();
        assert_eq!(view.layout(&console), &[0]);
        assert_eq!(rows_in(view.layout(&console), 10.0, 0.0, 100.0), 0..0);
    }

    #[test]
    fn filtering_and_counts() {
        let mut console = Console::default();
        console.info("Mounted 12 VPKs");
        console.warn("Missing material dev/foo");
        console.error("Leak detected");
        console.success("Compile finished");
        console.warn("Missing model props/bar.mdl");
        let mut view = ConsoleView::default();
        view.refresh(&console);
        assert_eq!(view.visible_rows(), 5);
        assert_eq!(view.counts(), [1, 1, 2, 1]);
        view.show[2] = false;
        view.refresh(&console);
        assert_eq!(view.visible_rows(), 3);
        view.show[2] = true;
        view.search = "MISSING".into();
        view.refresh(&console);
        assert_eq!(view.visible_rows(), 2);
        let text = view.visible_text(&console);
        assert!(text.contains("Missing material dev/foo") && text.contains("props/bar.mdl"), "{text}");
        console.warn("Missing sound");
        view.refresh(&console);
        assert_eq!(view.visible_rows(), 3);
        console.clear();
        view.refresh(&console);
        assert_eq!(view.visible_rows(), 0);
    }
}
