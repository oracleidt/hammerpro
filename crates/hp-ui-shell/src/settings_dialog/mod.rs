use egui::{RichText, Ui};
use hp_app::extend::{SettingsPage, page_order};
use hp_app::i18n::TrTable;
use hp_app::settings::GameSettings;
use hp_app::{Action, AppState, Severity, t};

use crate::common::EscapeGuard;
use crate::game_ui::GameJobs;
use crate::keys::{Capture, capture_from_events, is_clipboard_event};

mod blender;
mod brush;
mod compile;
mod content;
mod discord;
mod editing;
mod entities;
mod game;
mod general;
mod health;
mod helpers;
mod instances;
mod keyboard;
mod lab;
mod launch;
mod lighting;
mod mcp;
mod mesh;
mod placement;
mod render;
mod selection;
mod startup;
mod storage;
mod update;
mod viewport;
mod visibility;
mod vtf;
mod workflow;

pub use discord::shown_status as discord_shown_status;
pub use editing::sync_edit_options;
pub use keyboard::{
    apply_capture, apply_keymap_preset, key_search_id, preset_label_key, reset_all_bindings, set_binding,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SettingsTab {
    #[default]
    General,
    Game,
    Viewport,
    Editing,
    Compile,
    Keyboard,
    Package(&'static str),
}

impl SettingsTab {
    pub const ALL: [SettingsTab; 6] = [
        SettingsTab::General,
        SettingsTab::Game,
        SettingsTab::Viewport,
        SettingsTab::Editing,
        SettingsTab::Compile,
        SettingsTab::Keyboard,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsTab::General => t!("shell.settings.general"),
            SettingsTab::Game => t!("shell.settings.game"),
            SettingsTab::Viewport => t!("shell.settings.viewport"),
            SettingsTab::Editing => t!("shell.settings.editing"),
            SettingsTab::Compile => t!("shell.settings.compile"),
            SettingsTab::Keyboard => t!("shell.settings.keyboard"),
            SettingsTab::Package(id) => id,
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            SettingsTab::General => "⚙",
            SettingsTab::Game => "🎮",
            SettingsTab::Viewport => "🖵",
            SettingsTab::Editing => "✏",
            SettingsTab::Compile => "▶",
            SettingsTab::Keyboard => "⌨",
            SettingsTab::Package(_) => "⚙",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            SettingsTab::General => "general",
            SettingsTab::Game => "game",
            SettingsTab::Viewport => "viewport",
            SettingsTab::Editing => "editing",
            SettingsTab::Compile => "compile",
            SettingsTab::Keyboard => "keyboard",
            SettingsTab::Package(id) => id,
        }
    }

    pub fn from_id(id: &'static str) -> SettingsTab {
        SettingsTab::ALL.into_iter().find(|t| t.id() == id).unwrap_or(SettingsTab::Package(id))
    }

    fn order(self) -> i32 {
        match self {
            SettingsTab::General => page_order::GENERAL,
            SettingsTab::Game => page_order::GAME,
            SettingsTab::Viewport => page_order::VIEWPORT,
            SettingsTab::Editing => page_order::EDITING,
            SettingsTab::Compile => page_order::COMPILE,
            SettingsTab::Keyboard => page_order::KEYBOARD,
            SettingsTab::Package(_) => i32::MAX,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PageSection {
    Application,
    Game,
    Views,
    Editing,
    Compile,
    Integrations,
    Other,
}

impl PageSection {
    pub fn label(self) -> &'static str {
        match self {
            PageSection::Application => t!("shell.settings.section.application"),
            PageSection::Game => t!("shell.settings.section.game"),
            PageSection::Views => t!("shell.settings.section.views"),
            PageSection::Editing => t!("shell.settings.section.editing"),
            PageSection::Compile => t!("shell.settings.section.compile"),
            PageSection::Integrations => t!("shell.settings.section.integrations"),
            PageSection::Other => t!("shell.settings.section.other"),
        }
    }
}

pub fn page_section(id: &str) -> PageSection {
    match id {
        "general" | "keyboard" | "updates" | "storage" => PageSection::Application,
        "game" | "entities" => PageSection::Game,
        "viewport" | "render" | "lighting" | "helpers" | "lab" => PageSection::Views,
        "editing" | "selection" | "brush" | "mesh" | "instances" | "visibility" | "placement" => PageSection::Editing,
        "compile" | "health" => PageSection::Compile,
        "mcp" | "discord" | "blender" | "vtf" => PageSection::Integrations,
        _ => PageSection::Other,
    }
}

fn page_tables(id: &str) -> &'static [TrTable] {
    match id {
        "general" => &[general::TR],
        "game" => &[game::TR],
        "viewport" => &[viewport::TR],
        "editing" => &[editing::TR],
        "compile" => &[compile::TR],
        "keyboard" => &[keyboard::TR],
        "selection" => &[selection::TR],
        "brush" => &[brush::TR],
        "instances" => &[instances::TR],
        "visibility" => &[visibility::TR],
        "helpers" => &[helpers::TR],
        "mesh" => &[mesh::TR],
        "entities" => &[entities::TR],
        "render" => &[render::TR],
        "lighting" => &[lighting::TR],
        "placement" => &[placement::TR],
        "health" => &[health::TR],
        "lab" => &[lab::TR],
        "mcp" => &[mcp::TR],
        "discord" => &[discord::TR],
        "blender" => &[blender::TR],
        "vtf" => &[vtf::TR],
        _ => &[],
    }
}

type PageTexts = Vec<(String, String, bool)>;
type PageMatches = (String, hp_app::i18n::Lang, Vec<(SettingsTab, String)>);

#[derive(Default)]
struct PageIndex {
    lang: Option<hp_app::i18n::Lang>,
    pages: Vec<(SettingsTab, PageTexts)>,
}

impl PageIndex {
    fn build(&mut self, tabs: &[(SettingsTab, String)]) {
        let lang = hp_app::i18n::lang();
        if self.lang == Some(lang) && self.pages.len() == tabs.len() {
            return;
        }
        self.lang = Some(lang);
        self.pages = tabs
            .iter()
            .map(|(tab, label)| {
                let mut texts: PageTexts = vec![(crate::fuzzy::fold(label), label.clone(), false)];
                let mut keys: Vec<String> = Vec::new();
                for table in page_tables(tab.id()) {
                    keys.extend(table.iter().map(|(key, _, _)| (*key).to_owned()));
                }
                if *tab == SettingsTab::Keyboard {
                    keys.extend(Action::ALL.iter().map(|a| a.label_key()));
                }
                let english = hp_app::i18n::Lang::En;
                for key in &keys {
                    let text = hp_app::i18n::tr_in(key, lang);
                    texts.push((crate::fuzzy::fold(text), text.to_owned(), false));
                }
                if lang != english {
                    for key in &keys {
                        let text = hp_app::i18n::tr_in(key, english);
                        texts.push((crate::fuzzy::fold(text), text.to_owned(), true));
                    }
                }
                (*tab, texts)
            })
            .collect();
    }

    fn matches(&self, query: &str) -> Vec<(SettingsTab, String)> {
        let needle = crate::fuzzy::fold(query.trim());
        if needle.is_empty() {
            return Vec::new();
        }
        let word_start = |folded: &str| {
            folded
                .match_indices(needle.as_str())
                .any(|(i, _)| i == 0 || !folded.as_bytes()[i - 1].is_ascii_alphanumeric())
        };
        self.pages
            .iter()
            .filter_map(|(tab, texts)| {
                texts
                    .iter()
                    .filter(|(folded, _, _)| folded.contains(&needle))
                    .min_by_key(|(folded, text, english)| (*english, !word_start(folded), text.chars().count()))
                    .map(|(_, text, _)| (*tab, text.clone()))
            })
            .collect()
    }
}

fn package_pages() -> Vec<Box<dyn SettingsPage>> {
    let mut pages = Vec::new();
    pages.extend(selection::pages());
    pages.extend(brush::pages());
    pages.extend(workflow::pages());
    pages.extend(instances::pages());
    pages.extend(visibility::pages());
    pages.extend(helpers::pages());
    pages.extend(mesh::pages());
    pages.extend(entities::pages());
    pages.extend(render::pages());
    pages.extend(lighting::pages());
    pages.extend(placement::pages());
    pages.extend(compile::pages());
    pages.extend(startup::pages());
    pages.extend(launch::pages());
    pages.extend(content::pages());
    pages.extend(health::pages());
    pages.extend(lab::pages());
    pages.extend(mcp::pages());
    pages.extend(discord::pages());
    pages.extend(storage::pages());
    pages.extend(update::pages());
    pages.extend(blender::pages());
    pages.extend(vtf::pages());
    pages
}

pub(crate) const TABLES: &[TrTable] = &[
    TR,
    general::TR,
    game::TR,
    viewport::TR,
    editing::TR,
    compile::TR,
    keyboard::TR,
    selection::TR,
    brush::TR,
    workflow::TR,
    instances::TR,
    visibility::TR,
    helpers::TR,
    mesh::TR,
    entities::TR,
    render::TR,
    lighting::TR,
    placement::TR,
    startup::TR,
    launch::TR,
    content::TR,
    health::TR,
    lab::TR,
    mcp::TR,
    discord::TR,
    blender::TR,
    vtf::TR,
];

const CONFIRM_SECS: f64 = 4.0;

pub struct SettingsDialog {
    pub tab: SettingsTab,
    was_open: bool,
    applied_game: Option<GameSettings>,
    capture: Option<Action>,
    pub key_filter: String,
    ui_scale_edit: Option<f32>,
    background_pick: Option<std::sync::mpsc::Receiver<Option<std::path::PathBuf>>>,
    confirm_reset: Option<(SettingsTab, f64)>,
    escape: EscapeGuard,
    pub focus_key_search: bool,
    pages: Vec<Box<dyn SettingsPage>>,
    page_search: String,
    page_index: PageIndex,
    page_matches: Option<PageMatches>,
    tab_titles: Option<(hp_app::i18n::Lang, Vec<(SettingsTab, String)>)>,
}

impl Default for SettingsDialog {
    fn default() -> Self {
        Self {
            tab: SettingsTab::default(),
            was_open: false,
            applied_game: None,
            capture: None,
            key_filter: String::new(),
            ui_scale_edit: None,
            background_pick: None,
            confirm_reset: None,
            escape: EscapeGuard::default(),
            focus_key_search: false,
            pages: package_pages(),
            page_search: String::new(),
            page_index: PageIndex::default(),
            page_matches: None,
            tab_titles: None,
        }
    }
}

impl std::fmt::Debug for SettingsDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsDialog")
            .field("tab", &self.tab)
            .field("capture", &self.capture)
            .field("key_filter", &self.key_filter)
            .field("pages", &self.pages.iter().map(|p| p.id()).collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

fn window_id() -> egui::Id {
    egui::Id::unique("hp_shell_settings")
}

pub const DEFAULT_SIZE: [f32; 2] = [780.0, 560.0];

pub fn default_size(screen: egui::Vec2) -> [f32; 2] {
    [
        (screen.x * 0.6).clamp(DEFAULT_SIZE[0], 1100.0).min(screen.x - 40.0),
        (screen.y * 0.85).clamp(DEFAULT_SIZE[1], 900.0).min(screen.y - 40.0),
    ]
}
const PAGE_LIST_WIDTH: f32 = 184.0;

impl SettingsDialog {
    pub fn capturing(&self) -> bool {
        self.capture.is_some()
    }

    pub fn begin_capture(&mut self, action: Action) {
        self.capture = Some(action);
    }

    pub fn add_page(&mut self, page: Box<dyn SettingsPage>) {
        if !self.pages.iter().any(|p| p.id() == page.id()) {
            self.pages.push(page);
            self.tab_titles = None;
            self.page_index = PageIndex::default();
            self.page_matches = None;
        }
    }

    pub fn page_ids(&self) -> Vec<&'static str> {
        self.pages.iter().map(|p| p.id()).collect()
    }

    pub fn tabs(&self) -> Vec<SettingsTab> {
        let mut tabs: Vec<(i32, SettingsTab)> = SettingsTab::ALL.iter().map(|t| (t.order(), *t)).collect();
        tabs.extend(self.pages.iter().map(|p| (p.order(), SettingsTab::Package(p.id()))));
        tabs.sort_by_key(|(order, _)| *order);
        tabs.into_iter().map(|(_, t)| t).collect()
    }

    pub fn game_changed(&self, state: &AppState) -> bool {
        self.applied_game.as_ref().is_some_and(|g| *g != state.settings.game)
    }

    pub fn apply_game(&mut self, state: &mut AppState) {
        state.settings.sync_game();
        state.assets.remount(&state.settings);
        state.assets.reload_fgd(&state.settings);
        self.applied_game = Some(state.settings.game.clone());
        state.log(Severity::Info, t!("shell.game.reloading"));
    }

    pub fn process_capture(&mut self, ctx: &egui::Context, state: &mut AppState) {
        let Some(action) = self.capture else { return };
        let result = ctx.input(|i| capture_from_events(&i.raw.events));
        ctx.input_mut(|i| {
            i.events.retain(|e| !matches!(e, egui::Event::Key { .. } | egui::Event::Text(_)) && !is_clipboard_event(e))
        });
        match result {
            Capture::Pending => return,
            Capture::Cancel => self.capture = None,
            Capture::Set(sc) => {
                apply_capture(state, action, sc);
                self.capture = None;
            }
        }
        if let Some(id) = ctx.memory(|m| m.focused()) {
            ctx.memory_mut(|m| m.surrender_focus(id));
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState, jobs: &mut GameJobs) {
        let open_now = state.ui.show_settings;
        if open_now && !self.was_open {
            self.applied_game = Some(state.settings.game.clone());
            self.capture = None;
            self.confirm_reset = None;
            self.escape.reset();
        }
        if !open_now {
            if self.was_open {
                self.on_close(state);
            }
            self.was_open = false;
            return;
        }
        self.was_open = true;
        self.process_capture(ctx, state);

        let layer = crate::common::window_layer(window_id());
        if self.capture.is_none() && self.escape.should_close(ctx, layer) {
            state.ui.show_settings = false;
            self.on_close(state);
            self.was_open = false;
            return;
        }

        let mut open = true;
        let title = format!("⚙ {}", t!("shell.settings.title"));
        let size = default_size(ctx.content_rect().size());
        hp_app::chrome::Window::new(title)
            .id(window_id())
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(size)
            .pivot(egui::Align2::CENTER_CENTER)
            .default_pos(ctx.content_rect().center())
            .min_size([620.0, 380.0])
            .show(ctx, |ui| self.contents(ui, state, jobs));
        if !open {
            state.ui.show_settings = false;
            self.on_close(state);
            self.was_open = false;
        } else {
            self.escape.finish(ctx, layer);
        }
    }

    fn on_close(&mut self, state: &mut AppState) {
        self.capture = None;
        self.flush_ui_scale(state, None);
        if self.game_changed(state) {
            self.apply_game(state);
        }
    }

    fn flush_ui_scale(&mut self, state: &mut AppState, ctx: Option<&egui::Context>) {
        if let Some(scale) = self.ui_scale_edit.take() {
            state.settings.ui.ui_scale = scale;
            state.settings_dirty = true;
            if let Some(ctx) = ctx {
                hp_app::theme::apply(ctx, state.settings.ui.theme, scale);
            }
        }
    }

    fn tab_title(&self, tab: SettingsTab) -> String {
        match tab {
            SettingsTab::Package(id) => match self.pages.iter().find(|p| p.id() == id) {
                Some(page) => format!("{}  {}", page.icon(), page.label()),
                None => format!("{}  {id}", tab.icon()),
            },
            _ => format!("{}  {}", tab.icon(), tab.label()),
        }
    }

    fn tab_label(&self, tab: SettingsTab) -> String {
        match tab {
            SettingsTab::Package(id) => self.pages.iter().find(|p| p.id() == id).map_or(id.to_owned(), |p| p.label()),
            _ => tab.label().to_owned(),
        }
    }

    fn titles(&mut self) -> &[(SettingsTab, String)] {
        let lang = hp_app::i18n::lang();
        if self.tab_titles.as_ref().is_none_or(|(l, _)| *l != lang) {
            let mut tabs = self.tabs();
            tabs.sort_by_key(|t| page_section(t.id()));
            let titles = tabs.into_iter().map(|t| (t, self.tab_title(t))).collect();
            self.tab_titles = Some((lang, titles));
        }
        self.tab_titles.as_ref().map_or(&[], |(_, t)| t.as_slice())
    }

    pub fn search_pages(&mut self, query: &str) -> Vec<(SettingsTab, String)> {
        let tabs: Vec<SettingsTab> = self.titles().iter().map(|(t, _)| *t).collect();
        let labels: Vec<(SettingsTab, String)> = tabs.into_iter().map(|t| (t, self.tab_label(t))).collect();
        self.page_index.build(&labels);
        self.page_index.matches(query)
    }

    fn page_list(&mut self, ui: &mut Ui) {
        if hp_app::widgets::search_box(ui, &mut self.page_search, t!("shell.settings.search_hint")) {
            self.page_matches = None;
        }
        ui.add_space(4.0);
        let searching = !self.page_search.trim().is_empty();
        let lang = hp_app::i18n::lang();
        if searching
            && self.page_matches.as_ref().is_none_or(|(q, l, _)| q.as_str() != self.page_search.trim() || *l != lang)
        {
            let query = self.page_search.trim().to_owned();
            let found = self.search_pages(&query);
            self.page_matches = Some((query, lang, found));
        }
        let mut chosen = None;
        let current = self.tab;
        let weak = ui.visuals().weak_text_color();
        let titles = self.titles().to_vec();
        egui::ScrollArea::vertical().id_salt("hp_shell_settings_page_list").auto_shrink([false, false]).show(
            ui,
            |ui| {
                let width = ui.available_width();
                if !searching {
                    let mut section = None;
                    for (tab, title) in &titles {
                        let s = page_section(tab.id());
                        if section != Some(s) {
                            if section.is_some() {
                                ui.add_space(4.0);
                            }
                            section = Some(s);
                            ui.label(RichText::new(s.label()).small().strong().color(weak));
                        }
                        let b =
                            egui::Button::selectable(current == *tab, title.as_str()).min_size(egui::vec2(width, 24.0));
                        if ui.add(b).clicked() {
                            chosen = Some(*tab);
                        }
                    }
                    return;
                }
                let matches = self.page_matches.as_ref().map(|(_, _, m)| m.as_slice()).unwrap_or_default();
                if matches.is_empty() {
                    ui.label(RichText::new(t!("shell.settings.search_none")).weak().italics());
                }
                for (tab, text) in matches {
                    let title = titles.iter().find(|(t, _)| t == tab).map_or("", |(_, title)| title.as_str());
                    let b = egui::Button::selectable(current == *tab, title).min_size(egui::vec2(width, 24.0));
                    if ui.add(b).on_hover_text(text.as_str()).clicked() {
                        chosen = Some(*tab);
                    }
                    ui.add(egui::Label::new(RichText::new(text.as_str()).small().color(weak)).truncate());
                }
            },
        );
        if let Some(tab) = chosen {
            self.tab = tab;
            self.capture = None;
        }
    }

    fn contents(&mut self, ui: &mut Ui, state: &mut AppState, jobs: &mut GameJobs) {
        egui::Panel::bottom("hp_shell_settings_footer")
            .resizable(false)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { top: 6, ..Default::default() }))
            .show(ui, |ui| self.footer(ui, state));
        egui::Panel::left("hp_shell_settings_pages")
            .resizable(false)
            .exact_size(PAGE_LIST_WIDTH)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin { right: 6, top: 2, ..Default::default() }))
            .show(ui, |ui| self.page_list(ui));
        egui::CentralPanel::no_frame().show(ui, |ui| {
            egui::ScrollArea::both().id_salt(("hp_shell_settings_page", self.tab)).auto_shrink([false, false]).show(
                ui,
                |ui| {
                    egui::Frame::NONE.inner_margin(egui::Margin { left: 10, right: 8, ..Default::default() }).show(
                        ui,
                        |ui| match self.tab {
                            SettingsTab::General => self.general_page(ui, state),
                            SettingsTab::Game => game::page(ui, state, jobs),
                            SettingsTab::Viewport => viewport::page(ui, state),
                            SettingsTab::Editing => editing::page(ui, state),
                            SettingsTab::Compile => compile::page(ui, state, jobs),
                            SettingsTab::Keyboard => self.keyboard_page(ui, state),
                            SettingsTab::Package(id) => {
                                if let Some(page) = self.pages.iter_mut().find(|p| p.id() == id) {
                                    page.ui(ui, state);
                                }
                            }
                        },
                    );
                },
            );
        });
    }

    fn can_reset(&self) -> bool {
        match self.tab {
            SettingsTab::Game => false,
            SettingsTab::Package(id) => self.pages.iter().any(|p| p.id() == id && p.can_reset()),
            _ => true,
        }
    }

    fn footer(&mut self, ui: &mut Ui, state: &mut AppState) {
        ui.horizontal(|ui| {
            if self.can_reset() {
                let now = state.frame.time;
                let armed =
                    self.confirm_reset.is_some_and(|(tab, at)| tab == self.tab && (now - at).abs() < CONFIRM_SECS);
                let text =
                    if armed { t!("shell.settings.confirm_reset") } else { t!("shell.settings.restore_defaults") };
                let mut b = egui::Button::new(text);
                if armed {
                    b = b.fill(hp_app::theme::error().gamma_multiply(0.5));
                }
                if ui.add(b).on_hover_text(t!("shell.settings.restore_tip")).clicked() {
                    if armed {
                        self.reset_page(state, self.tab);
                        if self.tab == SettingsTab::General {
                            self.ui_scale_edit = None;
                            hp_app::theme::apply(ui.ctx(), state.settings.ui.theme, state.settings.ui.ui_scale);
                        }
                        self.confirm_reset = None;
                    } else {
                        self.confirm_reset = Some((self.tab, now));
                    }
                }
                if armed {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
                }
            }
            if self.game_changed(state) {
                let b = egui::Button::new(
                    RichText::new(format!("⟳ {}", t!("shell.settings.apply_reload")))
                        .color(hp_app::theme::on_accent())
                        .strong(),
                )
                .fill(hp_app::theme::accent());
                if ui.add(b).on_hover_text(t!("shell.settings.apply_reload_tip")).clicked() {
                    self.apply_game(state);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(t!("common.close")).clicked() {
                    state.ui.show_settings = false;
                }
                let note = RichText::new(t!("shell.settings.saved_auto")).small().color(ui.visuals().weak_text_color());
                ui.add(egui::Label::new(note).truncate());
            });
        });
    }

    fn reset_page(&mut self, state: &mut AppState, tab: SettingsTab) {
        match tab {
            SettingsTab::Package(id) => {
                if let Some(page) = self.pages.iter_mut().find(|p| p.id() == id) {
                    page.reset(state);
                    state.settings_dirty = true;
                }
            }
            builtin => reset_tab(state, builtin),
        }
    }
}

pub fn reset_tab(state: &mut AppState, tab: SettingsTab) {
    match tab {
        SettingsTab::General => general::reset(state),
        SettingsTab::Game | SettingsTab::Package(_) => {}
        SettingsTab::Viewport => viewport::reset(state),
        SettingsTab::Editing => editing::reset(state),
        SettingsTab::Compile => compile::reset(state),
        SettingsTab::Keyboard => reset_all_bindings(state),
    }
    state.settings_dirty = true;
}

pub(crate) const TR: TrTable = &[
    ("shell.settings.title", "Settings", "Paramètres"),
    ("shell.settings.general", "General", "Général"),
    ("shell.settings.game", "Game", "Jeu"),
    ("shell.settings.viewport", "Viewports", "Vues"),
    ("shell.settings.editing", "Editing", "Édition"),
    ("shell.settings.compile", "Compile", "Compilation"),
    ("shell.settings.keyboard", "Keyboard", "Clavier"),
    ("shell.settings.restore_defaults", "Restore defaults", "Valeurs par défaut"),
    ("shell.settings.section.application", "Application", "Application"),
    ("shell.settings.section.game", "Game and content", "Jeu et contenu"),
    ("shell.settings.section.views", "Views and rendering", "Vues et rendu"),
    ("shell.settings.section.editing", "Editing", "Édition"),
    ("shell.settings.section.compile", "Compile and check", "Compilation et vérification"),
    ("shell.settings.section.integrations", "Integrations", "Intégrations"),
    ("shell.settings.section.other", "Other", "Autres"),
    ("shell.settings.search_hint", "Find a setting", "Chercher un paramètre"),
    ("shell.settings.search_none", "No setting matches", "Aucun paramètre ne correspond"),
    ("shell.settings.confirm_reset", "Click again to confirm", "Cliquez à nouveau pour confirmer"),
    ("shell.settings.restore_tip", "Reset every option of this page", "Réinitialiser toutes les options de cette page"),
    ("shell.settings.apply_reload", "Apply and reload content", "Appliquer et recharger le contenu"),
    (
        "shell.settings.apply_reload_tip",
        "Mount the game content again with the new settings",
        "Remonter le contenu du jeu avec les nouveaux paramètres",
    ),
    (
        "shell.settings.saved_auto",
        "Changes are saved automatically",
        "Les modifications sont enregistrées automatiquement",
    ),
];

#[cfg(test)]
mod tests;
