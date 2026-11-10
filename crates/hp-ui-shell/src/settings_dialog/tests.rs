use egui::{Key, KeyboardShortcut, Modifiers};
use hp_app::Action;
use hp_app::settings::{CompileSettings, EditSettings, ViewSettings};

use super::*;
use crate::keys::{conflicts, is_customized};

#[test]
fn every_page_has_a_section() {
    let d = SettingsDialog::default();
    let tabs = d.tabs();
    assert!(tabs.len() >= 20, "{tabs:?}");
    for tab in &tabs {
        assert_ne!(page_section(tab.id()), PageSection::Other, "{tab:?} has no section");
    }
    assert_eq!(page_section("my_extension"), PageSection::Other);
}

#[test]
fn finding_a_setting_lists_its_pages() {
    let _lang = hp_app::testing::lang_lock();
    crate::register_translations();
    let mut d = SettingsDialog::default();
    let found = d.search_pages("vertical sync");
    assert!(found.iter().any(|(t, _)| *t == SettingsTab::Package("render")), "{found:?}");
    let found = d.search_pages("sync");
    let render = found.iter().find(|(t, _)| *t == SettingsTab::Package("render")).expect("render page");
    assert_eq!(render.1, "Vertical sync", "the shortest text that starts with the word is shown");
    let found = d.search_pages("UNDO");
    assert!(found.iter().any(|(t, _)| *t == SettingsTab::Keyboard), "{found:?}");
    assert!(found.iter().any(|(t, _)| *t == SettingsTab::Editing), "undo levels: {found:?}");
    assert!(d.search_pages("zzzz no such option").is_empty());
    assert!(d.search_pages("   ").is_empty());
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Fr);
    let found = d.search_pages("synchronisation verticale");
    assert!(found.iter().any(|(t, _)| *t == SettingsTab::Package("render")), "{found:?}");
    let found = d.search_pages("edition");
    assert!(found.iter().any(|(t, _)| *t == SettingsTab::Editing), "accents are ignored: {found:?}");
    hp_app::i18n::set_lang(hp_app::i18n::Lang::Ru);
    let found = d.search_pages("потоки");
    assert!(found.iter().any(|(t, _)| *t == SettingsTab::Compile), "{found:?}");
    let found = d.search_pages("vertical sync");
    assert!(found.iter().any(|(t, _)| *t == SettingsTab::Package("render")), "English names work too: {found:?}");
    hp_app::i18n::set_lang(hp_app::i18n::Lang::En);
}

#[test]
fn capture_stores_binding_and_cancels_triggered_actions() {
    let mut state = hp_app::testing::test_state();
    state.request(Action::SaveMap);
    state.request(Action::Copy);
    let sc = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
    apply_capture(&mut state, Action::RunMap, sc);
    assert_eq!(state.keys.shortcut(Action::RunMap), Some(sc));
    assert_eq!(state.settings.keybindings.get("run_map").map(String::as_str), Some("Ctrl+S"));
    assert!(!state.requested_actions.contains(&Action::SaveMap));
    assert!(state.requested_actions.contains(&Action::Copy));
    assert!(state.settings_dirty);
    assert!(conflicts(&state.keys).contains_key("Ctrl+S"));
    reset_all_bindings(&mut state);
    assert!(conflicts(&state.keys).is_empty());
    assert!(state.settings.keybindings.is_empty());
}

#[test]
fn capture_tool_shortcut_cancels_tool_switch() {
    let mut state = hp_app::testing::test_state();
    state.set_tool(hp_app::ToolId::Block);
    let sc = KeyboardShortcut::new(Modifiers::SHIFT, Key::B);
    apply_capture(&mut state, Action::Carve, sc);
    assert_eq!(state.requested_tool, None);
}

#[test]
fn clear_and_reset_binding() {
    let mut state = hp_app::testing::test_state();
    set_binding(&mut state, Action::Undo, None);
    assert_eq!(state.keys.shortcut(Action::Undo), None);
    assert_eq!(state.settings.keybindings.get("undo").map(String::as_str), Some(""));
    set_binding(&mut state, Action::Undo, Action::Undo.default_shortcut());
    assert!(!is_customized(&state.keys, Action::Undo));
}

#[test]
fn reset_pages() {
    let mut state = hp_app::testing::test_state();
    state.settings.view.fov_deg = 100.0;
    state.settings.view.render_mode = hp_app::settings::RenderModeSetting::Wireframe;
    reset_tab(&mut state, SettingsTab::Viewport);
    assert_eq!(state.settings.view.fov_deg, ViewSettings::default().fov_deg);
    assert_eq!(state.settings.view.render_mode, hp_app::settings::RenderModeSetting::Wireframe);

    state.settings.edit.undo_levels = 20;
    state.settings.edit.grid_size = 64.0;
    state.settings.edit.texture_lock = false;
    reset_tab(&mut state, SettingsTab::Editing);
    assert_eq!(state.settings.edit.undo_levels, EditSettings::default().undo_levels);
    assert_eq!(state.settings.edit.grid_size, 64.0, "grid size is not a page setting");
    assert!(state.doc.options().texture_lock);

    state.settings.compile.game_args = "-foo".into();
    reset_tab(&mut state, SettingsTab::Compile);
    assert_eq!(state.settings.compile, CompileSettings::default());

    state.settings.ui.ui_scale = 1.5;
    state.settings.ui.native_frames = true;
    state.settings.edit.autosave_minutes = 0;
    reset_tab(&mut state, SettingsTab::General);
    assert_eq!(state.settings.ui.ui_scale, 1.0);
    assert!(!state.settings.ui.native_frames, "Hammer Pro's window frames by default");
    assert_eq!(state.settings.edit.autosave_minutes, EditSettings::default().autosave_minutes);
}

#[test]
fn edit_options_sync_to_document() {
    let mut state = hp_app::testing::test_state();
    state.settings.edit.texture_lock = false;
    state.settings.edit.texture_scale_lock = true;
    state.settings.edit.undo_levels = 42;
    sync_edit_options(&mut state);
    assert!(!state.doc.options().texture_lock);
    assert!(state.doc.options().texture_scale_lock);
    assert_eq!(state.doc.undo_limits().max_steps, 42);
}

#[test]
fn game_change_detection() {
    let mut state = hp_app::testing::test_state();
    let mut dialog = SettingsDialog::default();
    assert!(!dialog.game_changed(&state));
    dialog.applied_game = Some(state.settings.game.clone());
    state.settings.game.mount_workshop = !state.settings.game.mount_workshop;
    assert!(dialog.game_changed(&state));
    dialog.apply_game(&mut state);
    assert!(!dialog.game_changed(&state));
}
