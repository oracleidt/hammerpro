pub mod background;
pub mod common;
pub mod console_view;
pub mod first_run;
pub mod fuzzy;
pub mod game_ui;
pub mod grid_rotate;
pub mod grids;
pub mod help;
pub mod icons;
pub mod jobs;
pub mod keys;
pub mod loading;
pub mod menu;
pub mod palette;
pub mod settings_dialog;
pub mod status;
pub mod toasts;
pub mod tool_strip;
pub mod toolbar;
mod tr;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_dedup;
#[cfg(test)]
mod tests_status;

use hp_app::AppState;

pub use settings_dialog::{SettingsTab, discord_shown_status};
pub use tr::{TABLES, TR};

pub fn register_translations() {
    for table in tr::all_tables() {
        hp_app::i18n::register(table);
    }
}

#[derive(Debug, Default)]
pub struct Shell {
    status: status::StatusBar,
    menu_extensions: Vec<hp_app::extend::MenuExtension>,
    console: console_view::ConsoleView,
    palette: palette::CommandPalette,
    settings: settings_dialog::SettingsDialog,
    wizard: first_run::FirstRunWizard,
    shortcuts: help::ShortcutsWindow,
    game_jobs: game_ui::GameJobs,
    loading: loading::LoadingOverlay,
    grid_manager: grids::GridManager,
}

impl Shell {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn menu_bar(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        menu::menu_bar_with(ui, state, &mut self.menu_extensions);
    }

    pub fn title_menus(&mut self, ui: &mut egui::Ui, state: &mut AppState) -> f32 {
        menu::title_menus_with(ui, state, &mut self.menu_extensions)
    }

    pub fn toolbar(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        toolbar::toolbar(ui, state);
    }

    pub fn tool_strip(&mut self, ui: &mut egui::Ui, state: &mut AppState) -> Option<egui::Rect> {
        tool_strip::tool_strip(ui, state)
    }

    pub fn tool_palette(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        tool_strip::tool_palette(ui, state);
    }

    pub fn selection_bar(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        toolbar::selection_bar(ui, state);
    }

    pub fn status_bar(&mut self, ui: &mut egui::Ui, state: &mut AppState, tool_hint: Option<String>) {
        let out = self.status.ui(ui, state, tool_hint);
        if let Some(page) = out.open_settings {
            self.open_settings(state, SettingsTab::from_id(page));
        }
    }

    pub fn loading_overlay(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        self.loading.ui(ui, state);
    }

    pub fn console(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        self.console.ui(ui, state);
    }

    pub fn dialogs(&mut self, ctx: &egui::Context, state: &mut AppState) {
        self.game_jobs.poll(state);
        if state.ui.show_settings {
            self.settings.process_capture(ctx, state);
        }
        self.palette.show(ctx, state);
        self.wizard.show(ctx, state, &mut self.game_jobs);
        self.settings.show(ctx, state, &mut self.game_jobs);
        if self.shortcuts.show(ctx, state).customize {
            state.ui.show_shortcuts = false;
            self.open_settings(state, SettingsTab::Keyboard);
        }
        help::about_window(ctx, state);
        self.grid_manager.show(ctx, state);
    }

    pub fn toasts(&mut self, ctx: &egui::Context, state: &mut AppState) {
        toasts::toasts(ctx, state);
    }

    pub fn open_settings(&mut self, state: &mut AppState, tab: SettingsTab) {
        self.settings.tab = tab;
        self.settings.focus_key_search = tab == SettingsTab::Keyboard;
        state.ui.show_settings = true;
    }

    pub fn capturing_shortcut(&self) -> bool {
        self.settings.capturing()
    }

    pub fn dialog_open(&self, state: &AppState) -> bool {
        let ui = &state.ui;
        ui.show_command_palette || ui.show_settings || ui.show_shortcuts || ui.show_about || ui.show_first_run
    }

    pub fn add_settings_page(&mut self, page: Box<dyn hp_app::extend::SettingsPage>) {
        self.settings.add_page(page);
    }

    pub fn add_status_segment(&mut self, segment: Box<dyn hp_app::extend::StatusSegment>) {
        self.status.add(segment);
    }

    pub fn add_menu_extension(&mut self, extension: hp_app::extend::MenuExtension) {
        self.menu_extensions.push(extension);
    }

    pub fn status(&mut self) -> &mut status::StatusBar {
        &mut self.status
    }

    pub fn palette(&mut self) -> &mut palette::CommandPalette {
        &mut self.palette
    }

    pub fn settings_dialog(&mut self) -> &mut settings_dialog::SettingsDialog {
        &mut self.settings
    }

    pub fn console_view(&mut self) -> &mut console_view::ConsoleView {
        &mut self.console
    }

    pub fn wizard(&mut self) -> &mut first_run::FirstRunWizard {
        &mut self.wizard
    }

    pub fn shortcuts_window(&mut self) -> &mut help::ShortcutsWindow {
        &mut self.shortcuts
    }
}
