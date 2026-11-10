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

#[derive(Debug)]
pub struct ConsoleView {
    pub show: [bool; 4],
    pub search: String,
    filtered: Vec<usize>,
    counts: [usize; 4],
    cache_key: Option<(u64, [bool; 4], String)>,
    labels: Option<HeaderLabels>,
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
        let row_height = ui.text_style_height(&TextStyle::Monospace).max(ui.text_style_height(&TextStyle::Body));
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
        egui::ScrollArea::vertical()
            .id_salt("hp_shell_console_rows")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show_rows(ui, row_height, total, |ui, range| {
                for row in range {
                    let Some(line) = self.filtered.get(row).and_then(|&i| state.console.lines().nth(i)) else {
                        continue;
                    };
                    let r = line_row(ui, line, row_height);
                    if let Some(action) = r {
                        copy_line = Some(action);
                    }
                }
            });
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

fn line_row(ui: &mut Ui, line: &ConsoleLine, row_height: f32) -> Option<String> {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_height), egui::Sense::CLICK);
    let mut text_clipped = false;
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        if response.hovered() {
            painter.rect_filled(rect, 2.0, visuals.widgets.hovered.weak_bg_fill.gamma_multiply(0.35));
        }
        let font = TextStyle::Monospace.resolve(ui.style());
        let small = TextStyle::Small.resolve(ui.style());
        let text_color = match line.severity {
            Severity::Info => visuals.text_color(),
            other => severity_color(visuals, other),
        };
        let y = rect.center().y;
        let mut x = rect.left() + 4.0;
        let r = painter.text(
            egui::pos2(x, y),
            egui::Align2::LEFT_CENTER,
            fmt_clock(line.time),
            small,
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
        let r = painter.text(egui::pos2(x, y), egui::Align2::LEFT_CENTER, &line.text, font, text_color);
        text_clipped = r.right() > rect.right();
    }
    let response = if text_clipped { response.on_hover_text(&line.text) } else { response };
    let mut copy = None;
    response.context_menu(|ui| {
        if ui.button(t!("common.copy_text")).clicked() {
            copy = Some(line.text.clone());
        }
        crate::common::close_on_escape(ui);
    });
    copy
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
