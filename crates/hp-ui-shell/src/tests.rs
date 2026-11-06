use std::sync::MutexGuard;

use egui::{Event, Key, KeyboardShortcut, Modifiers};
use hp_app::i18n::Lang;
use hp_app::testing::{run_ctx, run_ui, test_state};
use hp_app::{Action, AppState, Severity};

use crate::{SettingsTab, Shell};

pub(crate) fn lang_guard() -> MutexGuard<'static, ()> {
    hp_app::testing::lang_lock()
}

fn state() -> AppState {
    let s = test_state();
    crate::register_translations();
    s
}

fn run_frames(state: &mut AppState, frames: Vec<Vec<Event>>, mut f: impl FnMut(&egui::Context, &mut AppState)) {
    let ctx = egui::Context::default();
    for events in frames {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            f(&ctx, state);
        });
        out.textures_delta.clear();
    }
}

fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers }
}

fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn shell_sources() -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    sources(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    assert!(files.len() > 30, "sources not found");
    files
}

#[test]
fn translation_table_is_clean() {
    let mut seen = std::collections::HashSet::new();
    for table in crate::tr::all_tables() {
        let problems = hp_app::i18n::lint_table(table);
        assert!(problems.is_empty(), "{problems:#?}");
        for (k, _, _) in table {
            assert!(k.starts_with("shell."), "key {k} must use the shell. prefix");
            assert!(!hp_app::strings::TR.iter().any(|(c, _, _)| c == k), "key {k} collides with the core table");
            assert!(seen.insert(*k), "key {k} is in two tables");
        }
    }
}

#[test]
fn every_used_key_is_translated() {
    let tables = crate::tr::all_tables();
    let mut missing = Vec::new();
    for path in shell_sources() {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let mut rest = text.as_str();
        while let Some(pos) = rest.find("\"shell.") {
            let after = &rest[pos + 1..];
            let end = after.find('"').unwrap_or(after.len());
            let k = &after[..end];
            if !k.ends_with('.') && !tables.iter().any(|table| table.iter().any(|(t, _, _)| *t == k)) {
                missing.push(format!("{}: {k}", path.display()));
            }
            rest = &after[end.min(after.len())..];
        }
    }
    assert!(missing.is_empty(), "{missing:#?}");
}

#[test]
fn no_forbidden_dashes_in_sources() {
    for path in shell_sources() {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        assert!(!text.contains('\u{2014}') && !text.contains('\u{2013}'), "{}", path.display());
    }
}

#[test]
fn icon_glyphs_exist() {
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
    out.textures_delta.clear();
    let mut glyphs: Vec<&str> = vec![
        "✔", "▶", "−", "+", "⌖", "🎨", "⏶", "⏷", "🚪", "⏺", "⚙", "🎮", "🖵", "✏", "⌨", "🌙", "☀", "🔍", "➕", "✖", "⟲",
        "⟳", "🗑", "🗐", "🔨", "🐙", "📖", "🌐", "🗁", "▪", "⚠", "❌", "×", "ℹ", "💧", "🐛", "📋", "✂", "💾", "🗋", "⊞",
        "🔒", "👁", "🗀", "⌘", "#",
    ];
    for tool in hp_app::ToolId::ALL {
        glyphs.push(crate::common::tool_icon(tool));
    }
    glyphs.extend(crate::toolbar::button_glyphs());
    for a in Action::ALL {
        if let Some(icon) = crate::common::action_icon(*a) {
            glyphs.push(icon);
        }
    }
    for tab in SettingsTab::ALL {
        glyphs.push(tab.icon());
    }
    glyphs.push(crate::menu::DIRTY_MARKER);
    let font = egui::FontId::proportional(14.0);
    let missing: Vec<&str> = glyphs.into_iter().filter(|g| !ctx.fonts_mut(|f| f.has_glyphs(&font, g))).collect();
    assert!(missing.is_empty(), "missing glyphs: {missing:?}");
}

#[test]
fn panels_render_in_both_languages() {
    let _g = lang_guard();
    for lang in Lang::ALL {
        hp_app::i18n::set_lang(lang);
        let mut state = state();
        let mut shell = Shell::new();
        state.console.info("Mounted 3 VPKs");
        state.console.warn("Missing material dev/foo");
        state.console.error("Leak!");
        state.hover_world = Some(glam::DVec3::new(1.0, 2.5, -3.0));
        state.settings.view.show_fps = true;
        state.settings.recent_files.push("C:/maps/a.vmf".into());
        state.doc.select_all();
        run_ui(&mut state, |ui, state| {
            shell.menu_bar(ui, state);
            shell.title_menus(ui, state);
            shell.toolbar(ui, state);
            shell.tool_palette(ui, state);
            shell.status_bar(ui, state, Some("Drag to create a block".into()));
            shell.status_bar(ui, state, None);
            shell.console(ui, state);
        });
        assert!(state.requested_actions.is_empty(), "{:?}", state.requested_actions);
        assert!(state.requested_tool.is_none());
    }
    hp_app::i18n::set_lang(Lang::En);
}

#[test]
fn console_filters_through_shell() {
    let mut state = state();
    let mut shell = Shell::new();
    for i in 0..3000 {
        state.console.push(if i % 3 == 0 { Severity::Warning } else { Severity::Info }, format!("line {i}"));
    }
    shell.console_view().search = "line 29".into();
    run_ui(&mut state, |ui, state| shell.console(ui, state));
    assert_eq!(shell.console_view().visible_rows(), 1 + 10 + 100);
    shell.console_view().search.clear();
    shell.console_view().show[0] = false;
    run_ui(&mut state, |ui, state| shell.console(ui, state));
    assert_eq!(shell.console_view().visible_rows(), 1000);
}

#[test]
fn status_bar_handles_asset_states() {
    use hp_app::state::{AssetStatus, AssetsApi};
    struct Assets(AssetStatus);
    impl AssetsApi for Assets {
        fn status(&self) -> AssetStatus {
            self.0.clone()
        }
        fn fs(&self) -> Option<&hp_fs::GameFs> {
            None
        }
        fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd> {
            None
        }
        fn generation(&self) -> u64 {
            0
        }
        fn remount(&mut self, _: &hp_app::settings::Settings) {}
        fn reload_fgd(&mut self, _: &hp_app::settings::Settings) {}
        fn materials(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
            Default::default()
        }
        fn models(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
            Default::default()
        }
        fn sounds(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
            Default::default()
        }
        fn source_label(&self, _: u32) -> String {
            String::new()
        }
        fn material_info(&mut self, _: &str) -> Option<std::sync::Arc<hp_app::state::MaterialInfo>> {
            None
        }
        fn thumbnail(&mut self, _: &egui::Context, _: &str) -> hp_app::state::ThumbState {
            hp_app::state::ThumbState::Missing
        }
        fn model_info(&mut self, _: &str) -> hp_app::state::ModelState {
            hp_app::state::ModelState::Loading
        }
        fn model_preview(
            &mut self,
            _: &mut hp_app::Gfx,
            _: &str,
            _: &hp_app::state::PreviewCamera,
            _: [u32; 2],
        ) -> Option<egui::TextureId> {
            None
        }
        fn entity_bounds(&self, _: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
            None
        }
        fn invalidate_material(&mut self, _: &str) {}
        fn import_images(
            &mut self,
            _: &[std::path::PathBuf],
            _: &hp_app::state::ImportOptions,
        ) -> Vec<Result<String, String>> {
            Vec::new()
        }
        fn begin_frame(&mut self, _: &egui::Context, _: Option<&mut hp_app::Gfx>) {}
        fn sync_scene(&mut self, _: &mut hp_editor::Document, _: &mut hp_app::Gfx, _: &hp_app::state::SceneOptions) {}
        fn render_material_id(&mut self, _: &str) -> hp_render::MaterialId {
            0
        }
        fn status_line(&self) -> Option<String> {
            Some("Loading game content...".into())
        }
    }
    for status in
        [AssetStatus::Mounting, AssetStatus::NotConfigured, AssetStatus::Failed("boom".into()), AssetStatus::Ready]
    {
        let mut state = state();
        state.assets = Box::new(Assets(status));
        let mut shell = Shell::new();
        run_ui(&mut state, |ui, state| shell.status_bar(ui, state, None));
    }
}

#[test]
fn every_dialog_opens_without_panicking() {
    let _g = lang_guard();
    for lang in Lang::ALL {
        hp_app::i18n::set_lang(lang);
        let mut state = state();
        let mut shell = Shell::new();
        state.ui.show_command_palette = true;
        state.ui.show_shortcuts = true;
        state.ui.show_about = true;
        state.ui.show_first_run = true;
        state.settings.game.custom_texture_dirs.push(hp_app::settings::CustomDirSetting {
            path: "C:/Art/Bricks".into(),
            prefix: "custom/bricks".into(),
            enabled: true,
        });
        state.settings.game.extra_fgds.push("C:/fgd/extra.fgd".into());
        state.settings.compile.threads = Some(8);
        state.notify(Severity::Error, "Something failed");
        state.notify(Severity::Success, "Saved");
        for tab in SettingsTab::ALL {
            shell.open_settings(&mut state, tab);
            run_ctx(&mut state, |ctx, state| {
                shell.dialogs(ctx, state);
                shell.toasts(ctx, state);
            });
        }
        assert!(
            state.ui.show_settings && state.ui.show_command_palette && state.ui.show_shortcuts && state.ui.show_about
        );
        assert!(state.requested_actions.is_empty(), "{:?}", state.requested_actions);
    }
    hp_app::i18n::set_lang(Lang::En);
}

#[test]
fn wizard_steps_render() {
    let mut state = state();
    let mut shell = Shell::new();
    state.ui.show_first_run = true;
    for step in 0..crate::first_run::STEPS {
        run_ctx(&mut state, |ctx, state| {
            shell.dialogs(ctx, state);
            shell.wizard().step = step;
        });
    }
    assert!(state.ui.show_first_run);
}

#[test]
fn palette_typing_and_enter_runs_action() {
    let mut state = state();
    let mut shell = Shell::new();
    state.ui.show_command_palette = true;
    run_ctx(&mut state, |ctx, state| shell.dialogs(ctx, state));
    let typing = vec![Event::Text("select all".into())];
    run_frames(&mut state, vec![vec![], typing, vec![], vec![key(Key::Enter, Modifiers::NONE)]], |ctx, state| {
        shell.dialogs(ctx, state)
    });
    assert!(
        state.requested_actions.contains(&Action::SelectAll),
        "{:?} / {:?}",
        state.requested_actions,
        shell.palette().query
    );
    assert!(!state.ui.show_command_palette);
}

#[test]
fn palette_escape_closes() {
    let mut state = state();
    let mut shell = Shell::new();
    state.ui.show_command_palette = true;
    run_frames(&mut state, vec![vec![], vec![key(Key::Escape, Modifiers::NONE)]], |ctx, state| {
        shell.dialogs(ctx, state)
    });
    assert!(!state.ui.show_command_palette);
    assert!(state.requested_actions.is_empty());
}

#[test]
fn palette_arrow_navigation() {
    let mut state = state();
    let mut shell = Shell::new();
    state.ui.show_command_palette = true;
    run_frames(
        &mut state,
        vec![vec![], vec![key(Key::ArrowDown, Modifiers::NONE)], vec![key(Key::Enter, Modifiers::NONE)]],
        |ctx, state| shell.dialogs(ctx, state),
    );
    assert_eq!(state.requested_actions, vec![Action::OpenMap]);
}

#[test]
fn shortcut_capture_through_settings_window() {
    let mut state = state();
    let mut shell = Shell::new();
    shell.open_settings(&mut state, SettingsTab::Keyboard);
    run_ctx(&mut state, |ctx, state| shell.dialogs(ctx, state));
    shell.settings_dialog().begin_capture(Action::MergeConvex);
    assert!(shell.capturing_shortcut());
    let ctrl_shift = Modifiers { ctrl: true, command: true, shift: true, ..Modifiers::NONE };
    run_frames(&mut state, vec![vec![key(Key::K, ctrl_shift)]], |ctx, state| shell.dialogs(ctx, state));
    assert!(!shell.capturing_shortcut());
    let expected = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::K);
    assert_eq!(state.keys.shortcut(Action::MergeConvex), Some(expected));
    assert_eq!(state.settings.keybindings.get("merge_convex").map(String::as_str), Some("Ctrl+Shift+K"));
    assert!(state.ui.show_settings, "the settings window stays open");

    shell.settings_dialog().begin_capture(Action::Undo);
    run_frames(&mut state, vec![vec![key(Key::Escape, Modifiers::NONE)]], |ctx, state| shell.dialogs(ctx, state));
    assert!(!shell.capturing_shortcut());
    assert!(state.ui.show_settings);
    assert_eq!(state.keys.shortcut(Action::Undo), Action::Undo.default_shortcut());

    run_frames(&mut state, vec![vec![], vec![key(Key::Escape, Modifiers::NONE)]], |ctx, state| {
        shell.dialogs(ctx, state)
    });
    assert!(!state.ui.show_settings);
}

#[test]
fn closing_settings_applies_game_changes() {
    let _guard = lang_guard();
    hp_app::i18n::set_lang(Lang::En);
    let mut state = state();
    let mut shell = Shell::new();
    shell.open_settings(&mut state, SettingsTab::Game);
    run_ctx(&mut state, |ctx, state| shell.dialogs(ctx, state));
    state.settings.game.mount_workshop = !state.settings.game.mount_workshop;
    assert!(shell.settings_dialog().game_changed(&state));
    state.ui.show_settings = false;
    run_ctx(&mut state, |ctx, state| shell.dialogs(ctx, state));
    assert!(!shell.settings_dialog().game_changed(&state));
    let reloading = hp_app::i18n::tr("shell.game.reloading");
    assert!(state.console.lines().any(|l| l.text == reloading));
}

#[test]
fn shortcuts_window_filter_and_escape() {
    let mut state = state();
    let mut shell = Shell::new();
    state.ui.show_shortcuts = true;
    shell.shortcuts_window().filter = "save".into();
    run_ctx(&mut state, |ctx, state| shell.dialogs(ctx, state));
    assert!(state.ui.show_shortcuts);
    run_frames(&mut state, vec![vec![], vec![key(Key::Escape, Modifiers::NONE)]], |ctx, state| {
        shell.dialogs(ctx, state)
    });
    assert!(!state.ui.show_shortcuts);
}

#[test]
fn shortcuts_window_focuses_its_search_field() {
    let _g = lang_guard();
    hp_app::i18n::set_lang(Lang::En);
    let mut state = state();
    let mut shell = Shell::new();
    let ctx = egui::Context::default();
    let frame = |state: &mut AppState, shell: &mut Shell, events: Vec<Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut escape_left = false;
        let mut out = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            shell.dialogs(&ctx, state);
            escape_left = ctx.input(|i| i.key_pressed(Key::Escape));
        });
        out.textures_delta.clear();
        escape_left
    };
    frame(&mut state, &mut shell, vec![]);
    for round in 0..2 {
        state.ui.show_shortcuts = true;
        frame(&mut state, &mut shell, vec![]);
        assert_eq!(
            ctx.memory(|m| m.focused()),
            Some(crate::help::shortcuts_search_id()),
            "round {round}: the search field has the focus"
        );
        frame(&mut state, &mut shell, vec![Event::Text("undo".into())]);
        assert_eq!(shell.shortcuts_window().filter, "undo", "round {round}");
        assert!(state.requested_actions.is_empty());
        let left = frame(&mut state, &mut shell, vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(!state.ui.show_shortcuts, "round {round}: Escape closes the window");
        assert!(!left, "round {round}: Escape was consumed");
        frame(&mut state, &mut shell, vec![]);
        assert_eq!(ctx.memory(|m| m.focused()), None, "round {round}: no focus left behind");
        shell.shortcuts_window().filter.clear();
    }
    shell.open_settings(&mut state, SettingsTab::Keyboard);
    frame(&mut state, &mut shell, vec![]);
    assert_eq!(ctx.memory(|m| m.focused()), Some(crate::settings_dialog::key_search_id()));
    frame(&mut state, &mut shell, vec![Event::Text("redo".into())]);
    assert_eq!(shell.settings_dialog().key_filter, "redo");
}

#[test]
fn about_closes_with_escape() {
    let mut state = state();
    let mut shell = Shell::new();
    state.ui.show_about = true;
    run_frames(&mut state, vec![vec![], vec![key(Key::Escape, Modifiers::NONE)]], |ctx, state| {
        shell.dialogs(ctx, state)
    });
    assert!(!state.ui.show_about);
}

#[test]
fn console_rows_and_toasts_are_not_tab_stops() {
    fn tab_stops(lines: usize, toasts: usize) -> usize {
        let mut state = state();
        let mut shell = Shell::new();
        for i in 0..lines {
            state.console.info(format!("line {i}"));
        }
        for i in 0..toasts {
            state.notify(Severity::Info, format!("toast {i}"));
        }
        let mut frames = vec![vec![]];
        frames.extend((0..40).map(|_| vec![key(Key::Tab, Modifiers::NONE)]));
        let mut seen = std::collections::HashSet::new();
        let ctx = egui::Context::default();
        for events in frames {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| shell.console(ui, &mut state));
                let ctx = ui.ctx().clone();
                shell.toasts(&ctx, &mut state);
            });
            out.textures_delta.clear();
            if let Some(id) = ctx.memory(|m| m.focused()) {
                seen.insert(id);
            }
        }
        seen.len()
    }
    let few = tab_stops(2, 0);
    assert!(few > 0, "the console has Tab stops (search field, buttons)");
    assert_eq!(tab_stops(12, 3), few, "lines and toasts must not be Tab stops");
}

#[test]
fn toasts_render() {
    let mut state = state();
    let mut shell = Shell::new();
    for sev in [Severity::Info, Severity::Success, Severity::Warning, Severity::Error] {
        state.toasts.push(sev, format!("{sev:?} message with a fairly long text that wraps on two lines"));
    }
    run_ctx(&mut state, |ctx, state| shell.toasts(ctx, state));
    assert_eq!(state.toasts.live().len(), 4);
}

#[test]
fn language_switch_helper() {
    let _g = lang_guard();
    let mut state = state();
    crate::menu::switch_language(&mut state, Lang::Fr);
    assert_eq!(state.settings.language, Lang::Fr);
    assert_eq!(hp_app::i18n::lang(), Lang::Fr);
    assert!(state.settings_dirty && state.settings.language_chosen);
    assert_eq!(hp_app::t!("shell.settings.title"), "Paramètres");
    crate::menu::switch_language(&mut state, Lang::En);
    assert_eq!(hp_app::t!("shell.settings.title"), "Settings");
}

#[test]
fn open_settings_on_a_page() {
    let mut state = state();
    let mut shell = Shell::new();
    shell.open_settings(&mut state, SettingsTab::Game);
    assert!(state.ui.show_settings);
    assert_eq!(shell.settings_dialog().tab, SettingsTab::Game);
}

struct Harness {
    ctx: egui::Context,
    nodes: Vec<(String, egui::Rect)>,
    escape_left: bool,
    screen: egui::Vec2,
}

impl Harness {
    fn new() -> Self {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        Self { ctx, nodes: Vec::new(), escape_left: false, screen: egui::vec2(1600.0, 900.0) }
    }

    fn frame(&mut self, state: &mut AppState, shell: &mut Shell, events: Vec<Event>) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.screen)),
            events,
            ..Default::default()
        };
        let mut escape_left = false;
        let mut out = self.ctx.run_ui(input, |ui| {
            egui::Panel::top("top").show(ui, |ui| shell.menu_bar(ui, state));
            let ctx = ui.ctx().clone();
            shell.dialogs(&ctx, state);
            escape_left = ctx.input(|i| i.key_pressed(Key::Escape));
        });
        out.textures_delta.clear();
        self.escape_left = escape_left;
        self.nodes.clear();
        if let Some(update) = out.platform_output.accesskit_update {
            for (_, node) in update.nodes {
                let text = [node.label(), node.value()].into_iter().flatten().collect::<Vec<_>>().join(" ");
                if let Some(b) = node.bounds() {
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(b.x0 as f32, b.y0 as f32),
                        egui::pos2(b.x1 as f32, b.y1 as f32),
                    );
                    self.nodes.push((text, rect));
                }
            }
        }
    }

    fn find(&self, text: &str) -> Option<egui::Rect> {
        let by = |f: &dyn Fn(&str) -> bool| self.nodes.iter().find(|(t, _)| f(t.trim())).map(|(_, r)| *r);
        by(&|t| t == text).or_else(|| by(&|t| t.ends_with(text))).or_else(|| by(&|t| t.contains(text)))
    }

    fn click(&mut self, state: &mut AppState, shell: &mut Shell, text: &str) {
        let rect = self.find(text).unwrap_or_else(|| {
            panic!("no widget {text:?} in {:?}", self.nodes.iter().map(|(t, _)| t).collect::<Vec<_>>())
        });
        let pos = rect.center();
        self.frame(state, shell, vec![Event::PointerMoved(pos)]);
        let button = egui::PointerButton::Primary;
        let press = Event::PointerButton { pos, button, pressed: true, modifiers: Modifiers::NONE };
        self.frame(state, shell, vec![press]);
        let release = Event::PointerButton { pos, button, pressed: false, modifiers: Modifiers::NONE };
        self.frame(state, shell, vec![release]);
        self.frame(state, shell, vec![]);
    }
}

#[test]
fn language_menu_follows_the_displayed_language() {
    let _g = lang_guard();
    let mut state = state();
    let mut shell = Shell::new();
    state.settings.language = Lang::En;
    hp_app::i18n::set_lang(Lang::Fr);
    let mut h = Harness::new();
    h.frame(&mut state, &mut shell, vec![]);
    h.click(&mut state, &mut shell, "Affichage");
    h.click(&mut state, &mut shell, "Langue");
    h.click(&mut state, &mut shell, "English");
    let shown = hp_app::i18n::lang();
    hp_app::i18n::set_lang(Lang::En);
    assert_eq!(shown, Lang::En, "choosing English must switch the UI");
    assert_eq!(state.settings.language, Lang::En);
    assert!(state.settings.language_chosen && state.settings_dirty);
}

#[test]
fn settings_language_combo_follows_the_displayed_language() {
    let _g = lang_guard();
    let mut state = state();
    let mut shell = Shell::new();
    state.settings.language = Lang::En;
    hp_app::i18n::set_lang(Lang::Fr);
    shell.open_settings(&mut state, SettingsTab::General);
    let mut h = Harness::new();
    h.frame(&mut state, &mut shell, vec![]);
    h.frame(&mut state, &mut shell, vec![]);
    let combo_shows_french = h.find("Français").is_some();
    h.click(&mut state, &mut shell, "Français");
    h.click(&mut state, &mut shell, "English");
    let shown = hp_app::i18n::lang();
    hp_app::i18n::set_lang(Lang::En);
    assert!(combo_shows_french, "the combo must show the displayed language");
    assert_eq!(shown, Lang::En);
    assert!(state.ui.show_settings, "choosing in the combo keeps the window open");
}

#[test]
fn escape_closes_menus_and_is_consumed() {
    let _g = lang_guard();
    hp_app::i18n::set_lang(Lang::En);
    let mut state = state();
    let mut shell = Shell::new();
    let mut h = Harness::new();
    h.frame(&mut state, &mut shell, vec![]);
    h.click(&mut state, &mut shell, "File");
    assert!(egui::Popup::is_any_open(&h.ctx), "the File menu opens");
    assert!(h.find("Save as").is_some(), "File menu entries are visible");
    h.frame(&mut state, &mut shell, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(!h.escape_left, "Escape must be consumed by the menu");
    h.frame(&mut state, &mut shell, vec![]);
    assert!(!egui::Popup::is_any_open(&h.ctx), "Escape closes the menu");
    assert!(h.find("Save as").is_none());
}

#[test]
fn dialogs_consume_escape() {
    let _g = lang_guard();
    hp_app::i18n::set_lang(Lang::En);
    for which in 0..5 {
        let mut state = state();
        let mut shell = Shell::new();
        match which {
            0 => state.ui.show_command_palette = true,
            1 => state.ui.show_settings = true,
            2 => state.ui.show_shortcuts = true,
            3 => state.ui.show_about = true,
            _ => state.ui.show_first_run = true,
        }
        state.settings.language_chosen = false;
        assert!(shell.dialog_open(&state));
        let mut h = Harness::new();
        h.frame(&mut state, &mut shell, vec![]);
        h.frame(&mut state, &mut shell, vec![]);
        h.frame(&mut state, &mut shell, vec![key(Key::Escape, Modifiers::NONE)]);
        assert!(!h.escape_left, "dialog {which} leaked Escape");
        let ui = &state.ui;
        assert!(
            !(ui.show_command_palette || ui.show_settings || ui.show_shortcuts || ui.show_about || ui.show_first_run),
            "dialog {which} stayed open"
        );
        assert!(!shell.dialog_open(&state));
        if which == 4 {
            assert!(state.settings.language_chosen, "Escape skips the wizard like the Skip button");
        }
        assert!(state.requested_actions.is_empty(), "{:?}", state.requested_actions);
    }
}

#[test]
fn escape_guard_closes_from_a_text_field() {
    let ctx = egui::Context::default();
    let window = egui::Id::unique("hp_shell_test_window");
    let field = egui::Id::unique("hp_shell_test_field");
    let mut guard = crate::common::EscapeGuard::default();
    let mut text = String::new();
    let mut closed = Vec::new();
    let mut leaked = Vec::new();
    let mut open = true;
    let escape = || vec![key(Key::Escape, Modifiers::NONE)];
    for (frame, events) in [vec![], vec![], escape(), escape()].into_iter().enumerate() {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            let layer = crate::common::window_layer(window);
            let close = open && guard.should_close(&ctx, layer);
            open &= !close;
            if open {
                egui::Window::new("dialog").id(window).show(&ctx, |ui| {
                    ui.add(egui::TextEdit::singleline(&mut text).id(field));
                });
                if frame == 0 {
                    ctx.memory_mut(|m| m.request_focus(field));
                }
                if frame == 1 {
                    assert!(ctx.text_edit_focused(), "the field has the focus");
                }
                guard.finish(&ctx, layer);
            }
            closed.push(close);
            leaked.push(ctx.input(|i| i.key_pressed(Key::Escape)));
        });
        out.textures_delta.clear();
    }
    assert_eq!(closed, [false, false, true, false], "the first Escape closes the dialog");
    assert_eq!(leaked, [false, false, false, true], "consumed while open, left to the app once closed");
}

#[test]
fn escape_closes_the_top_most_dialog_only() {
    let _g = lang_guard();
    hp_app::i18n::set_lang(Lang::En);
    let ctx = egui::Context::default();
    let other = egui::Id::unique("hp_shell_test_other_window");
    let mut other_open = false;
    let frame = |state: &mut AppState, shell: &mut Shell, other_open: &mut bool, events: Vec<Event>| {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            let ctx = ui.ctx().clone();
            shell.dialogs(&ctx, state);
            if *other_open {
                let layer = crate::common::window_layer(other);
                if crate::common::is_top_dialog(&ctx, layer)
                    && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
                {
                    *other_open = false;
                } else {
                    egui::Window::new("Other").id(other).default_pos([100.0, 100.0]).show(&ctx, |ui| {
                        ui.label("other component");
                    });
                }
            }
        });
        out.textures_delta.clear();
    };
    let escape = || vec![Event::PointerMoved(egui::pos2(5.0, 890.0)), key(Key::Escape, Modifiers::NONE)];
    let mut state = state();
    let mut shell = Shell::new();
    state.ui.show_settings = true;
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    state.ui.show_shortcuts = true;
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    frame(&mut state, &mut shell, &mut other_open, escape());
    assert!(!state.ui.show_shortcuts && state.ui.show_settings, "the shortcuts window was on top");
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    frame(&mut state, &mut shell, &mut other_open, escape());
    assert!(!state.ui.show_settings, "then the settings window");

    state.ui.show_about = true;
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    state.ui.show_settings = true;
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    frame(&mut state, &mut shell, &mut other_open, escape());
    assert!(!state.ui.show_settings && state.ui.show_about, "the settings window was on top");

    other_open = true;
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    frame(&mut state, &mut shell, &mut other_open, escape());
    assert!(!other_open, "the window on top closed");
    assert!(state.ui.show_about, "the shell window under it stays open");
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    frame(&mut state, &mut shell, &mut other_open, escape());
    assert!(!state.ui.show_about);

    hp_app::widgets::register_palette_window(&ctx, other);
    state.ui.show_settings = true;
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    other_open = true;
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    frame(&mut state, &mut shell, &mut other_open, vec![]);
    frame(&mut state, &mut shell, &mut other_open, escape());
    assert!(!state.ui.show_settings, "the settings window under the palette closed");
    assert!(other_open, "the palette stays");
}

#[test]
fn escape_closes_a_combo_inside_a_dialog_first() {
    let _g = lang_guard();
    hp_app::i18n::set_lang(Lang::En);
    let mut state = state();
    let mut shell = Shell::new();
    shell.open_settings(&mut state, SettingsTab::General);
    let mut h = Harness::new();
    h.frame(&mut state, &mut shell, vec![]);
    h.frame(&mut state, &mut shell, vec![]);
    h.click(&mut state, &mut shell, "English");
    assert!(egui::Popup::is_any_open(&h.ctx), "the language combo is open");
    h.frame(&mut state, &mut shell, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(!h.escape_left, "the combo's Escape is consumed");
    h.frame(&mut state, &mut shell, vec![]);
    assert!(!egui::Popup::is_any_open(&h.ctx), "Escape closed the combo");
    assert!(state.ui.show_settings, "and only the combo");
    h.frame(&mut state, &mut shell, vec![key(Key::Escape, Modifiers::NONE)]);
    assert!(!h.escape_left);
    assert!(!state.ui.show_settings, "the next Escape closes the window");
}

#[test]
fn settings_window_keeps_its_size() {
    let _g = lang_guard();
    let id = egui::Id::unique("hp_shell_settings");
    for lang in Lang::ALL {
        hp_app::i18n::set_lang(lang);
        for tab in SettingsTab::ALL {
            let mut state = state();
            state.settings.game.gmod_root = Some("C:/Program Files (x86)/Steam/steamapps/common/GarrysMod".into());
            let mut shell = Shell::new();
            shell.open_settings(&mut state, tab);
            let ctx = egui::Context::default();
            hp_app::theme::apply(&ctx, hp_app::settings::ThemeChoice::Dark, 1.0);
            let mut sizes = Vec::new();
            for frame in 0..120 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0))),
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| {
                    let ctx = ui.ctx().clone();
                    shell.dialogs(&ctx, &mut state);
                });
                out.textures_delta.clear();
                if frame == 5 || frame == 119 {
                    sizes.push(ctx.memory(|m| m.area_rect(id)).map(|r| r.size()));
                }
            }
            let (Some(early), Some(late)) = (sizes[0], sizes[1]) else {
                panic!("settings window not shown ({lang:?}, {tab:?})");
            };
            assert!((late - early).length() < 1.0, "{lang:?} {tab:?}: the window grew from {early:?} to {late:?}");
            let default = egui::Vec2::from(crate::settings_dialog::default_size(egui::vec2(1600.0, 900.0)));
            assert!(
                (late - default).length() < 1.0,
                "{lang:?} {tab:?}: {late:?} instead of the default size {default:?}"
            );
        }
    }
    hp_app::i18n::set_lang(Lang::En);
}

#[test]
fn french_fly_keys_mention_azerty() {
    crate::register_translations();
    let fr = hp_app::i18n::tr_in("shell.help.ctl_wasd", Lang::Fr);
    assert!(fr.contains("Z Q S D") && fr.contains("A E") && fr.contains("QWERTY"), "{fr}");
}

#[test]
fn palette_labels_follow_french_typography() {
    let _g = lang_guard();
    let mut state = state();
    state.settings.recent_files.push("C:/maps/gm_test.vmf".into());
    hp_app::i18n::set_lang(Lang::Fr);
    let entries = crate::palette::build_entries(&state);
    hp_app::i18n::set_lang(Lang::En);
    let labels: Vec<&str> = entries.iter().map(|e| e.label.as_str()).collect();
    assert!(labels.contains(&"Langue : English"), "{labels:?}");
    assert!(labels.contains(&"Ouvrir : gm_test.vmf"), "{labels:?}");
}

#[test]
fn shortcut_capture_of_clipboard_keys_and_azerty_digits() {
    let mut state = state();
    let mut shell = Shell::new();
    shell.open_settings(&mut state, SettingsTab::Keyboard);
    run_ctx(&mut state, |ctx, state| shell.dialogs(ctx, state));

    shell.settings_dialog().begin_capture(Action::RunMap);
    let mut left = Vec::new();
    run_frames(&mut state, vec![vec![Event::Copy]], |ctx, state| {
        shell.dialogs(ctx, state);
        left = ctx.input(|i| i.events.clone());
    });
    assert!(!shell.capturing_shortcut());
    assert_eq!(state.keys.shortcut(Action::RunMap), Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::C)));
    assert_eq!(state.settings.keybindings.get("run_map").map(String::as_str), Some("Ctrl+C"));
    assert!(!left.iter().any(crate::keys::is_clipboard_event), "nothing else may copy: {left:?}");
    assert!(crate::keys::conflicts(&state.keys).contains_key("Ctrl+C"), "Copy keeps Ctrl+C: a visible conflict");

    shell.settings_dialog().begin_capture(Action::Paste);
    run_frames(&mut state, vec![vec![Event::Paste("clipboard text".into())]], |ctx, state| shell.dialogs(ctx, state));
    assert_eq!(state.keys.shortcut(Action::Paste), Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::V)));

    shell.settings_dialog().begin_capture(Action::MergeConvex);
    let alt_4 = Event::Key {
        key: Key::Quote,
        physical_key: Some(Key::Num4),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::ALT,
    };
    run_frames(&mut state, vec![vec![alt_4.clone()]], |ctx, state| shell.dialogs(ctx, state));
    let expected = KeyboardShortcut::new(Modifiers::ALT, Key::Num4);
    assert_eq!(state.keys.shortcut(Action::MergeConvex), Some(expected));
    assert_eq!(state.settings.keybindings.get("merge_convex").map(String::as_str), Some("Alt+4"));
    let mut events = vec![alt_4];
    assert_eq!(state.keys.take_triggered(&mut events), vec![Action::MergeConvex]);
}

#[test]
fn tooltips_show_localized_shortcut_labels() {
    let _g = lang_guard();
    let state = state();
    let save_as = KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S);
    let delete = KeyboardShortcut::new(Modifiers::NONE, Key::Delete);
    hp_app::i18n::set_lang(Lang::Fr);
    let fr = crate::common::tooltip_with_shortcut("Enregistrer sous", Some(save_as));
    let fr_delete = crate::common::tooltip_with_shortcut("Supprimer", Some(delete));
    let action = crate::common::action_tooltip(&state, Action::SaveMapAs);
    hp_app::i18n::set_lang(Lang::En);
    assert_eq!(fr, "Enregistrer sous (Ctrl+Maj+S)");
    assert_eq!(fr_delete, "Supprimer (Suppr)");
    assert!(action.ends_with("(Ctrl+Maj+S)"), "{action}");
    assert_eq!(crate::common::tooltip_with_shortcut("Save as", Some(save_as)), "Save as (Ctrl+Shift+S)");
    assert_eq!(crate::common::tooltip_with_shortcut("Save as", None), "Save as");
}

struct FsAssets {
    inner: hp_app::testing::MockAssets,
    fs: hp_fs::GameFs,
}

impl hp_app::state::AssetsApi for FsAssets {
    fn status(&self) -> hp_app::state::AssetStatus {
        hp_app::state::AssetStatus::Ready
    }
    fn fs(&self) -> Option<&hp_fs::GameFs> {
        Some(&self.fs)
    }
    fn fgd(&self) -> Option<&hp_fgd::ResolvedFgd> {
        None
    }
    fn generation(&self) -> u64 {
        self.fs.generation()
    }
    fn remount(&mut self, _: &hp_app::settings::Settings) {}
    fn reload_fgd(&mut self, _: &hp_app::settings::Settings) {}
    fn materials(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
        self.fs.materials()
    }
    fn models(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
        self.fs.models()
    }
    fn sounds(&self) -> std::sync::Arc<Vec<hp_fs::AssetEntry>> {
        self.fs.sounds()
    }
    fn source_label(&self, source: u32) -> String {
        self.inner.source_label(source)
    }
    fn material_info(&mut self, name: &str) -> Option<std::sync::Arc<hp_app::state::MaterialInfo>> {
        self.inner.material_info(name)
    }
    fn thumbnail(&mut self, ctx: &egui::Context, name: &str) -> hp_app::state::ThumbState {
        self.inner.thumbnail(ctx, name)
    }
    fn model_info(&mut self, path: &str) -> hp_app::state::ModelState {
        self.inner.model_info(path)
    }
    fn model_preview(
        &mut self,
        gfx: &mut hp_app::Gfx,
        path: &str,
        camera: &hp_app::state::PreviewCamera,
        size_px: [u32; 2],
    ) -> Option<egui::TextureId> {
        self.inner.model_preview(gfx, path, camera, size_px)
    }
    fn entity_bounds(&self, entity: &hp_vmf::Entity) -> Option<hp_geom::Aabb> {
        self.inner.entity_bounds(entity)
    }
    fn invalidate_material(&mut self, name: &str) {
        self.inner.invalidate_material(name);
    }
    fn import_images(
        &mut self,
        files: &[std::path::PathBuf],
        opts: &hp_app::state::ImportOptions,
    ) -> Vec<Result<String, String>> {
        self.inner.import_images(files, opts)
    }
    fn begin_frame(&mut self, ctx: &egui::Context, gfx: Option<&mut hp_app::Gfx>) {
        self.inner.begin_frame(ctx, gfx);
    }
    fn sync_scene(&mut self, doc: &mut hp_editor::Document, gfx: &mut hp_app::Gfx, opts: &hp_app::state::SceneOptions) {
        self.inner.sync_scene(doc, gfx, opts);
    }
    fn render_material_id(&mut self, name: &str) -> hp_render::MaterialId {
        self.inner.render_material_id(name)
    }
    fn status_line(&self) -> Option<String> {
        None
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("hp-ui-shell")
        .join("scratch")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[test]
fn settings_custom_folders_show_the_detected_layout() {
    let _g = lang_guard();
    hp_app::i18n::set_lang(Lang::En);
    let root = scratch("custom_layout");
    let pack = root.join("Pack Folder");
    let _ = std::fs::create_dir_all(&pack);
    let vmt: &[u8] = b"LightmappedGeneric { $basetexture \"mypack/brick01\" }";
    let _ = std::fs::write(pack.join("brick01.vmt"), vmt);
    let _ = std::fs::write(pack.join("brick01.vtf"), b"VTF");
    let game = root.join("Game Folder");
    let walls = game.join("materials").join("walls");
    let _ = std::fs::create_dir_all(&walls);
    let _ = std::fs::write(walls.join("w.vmt"), b"LightmappedGeneric { }");

    let suggestion = hp_fs::suggest_prefix(&pack);
    let fs = hp_fs::GameFs::empty();
    assert!(fs.add_custom_dir(hp_fs::CustomDir::with_prefix(&pack, suggestion.clone())).is_ok());
    assert!(fs.add_custom_dir(hp_fs::CustomDir::new(&game)).is_ok());
    let layout = fs.custom_layout_of_path(&pack);
    assert_eq!(layout.as_ref().map(|l| (l.mode, l.prefix.as_str())), Some((hp_fs::CustomMode::Detected, "mypack")));

    let mut settings = hp_app::settings::Settings::default();
    for (path, prefix) in [(&pack, suggestion.clone()), (&game, hp_fs::suggest_prefix(&game))] {
        settings.game.custom_texture_dirs.push(hp_app::settings::CustomDirSetting {
            path: path.clone(),
            prefix,
            enabled: true,
        });
    }
    let mut state = AppState::new(settings, Box::new(FsAssets { inner: Default::default(), fs }));
    crate::register_translations();
    let mut shell = Shell::new();
    shell.open_settings(&mut state, SettingsTab::Game);
    let mut h = Harness::new();
    h.frame(&mut state, &mut shell, vec![]);
    h.frame(&mut state, &mut shell, vec![]);
    let prefix = |state: &AppState| state.settings.game.custom_texture_dirs[0].prefix.clone();
    let texts = |h: &Harness| h.nodes.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>();
    let has = |h: &Harness, text: &str| h.nodes.iter().any(|(t, _)| t.trim() == text);
    assert!(has(&h, "mypack"), "the detected prefix is shown: {:?}", texts(&h));
    assert!(has(&h, "detected"), "with the marker: {:?}", texts(&h));
    assert!(has(&h, "materials/"), "the game folder shows its layout: {:?}", texts(&h));
    assert!(!texts(&h).iter().any(|t| t.trim() == suggestion), "the unused suggestion is not shown");
    assert_eq!(prefix(&state), suggestion, "showing it changes nothing");
    assert!(!state.settings_dirty);

    let ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
    h.click(&mut state, &mut shell, "mypack");
    assert!(h.ctx.memory(|m| m.focused()).is_some(), "the field has focus");
    h.frame(&mut state, &mut shell, vec![key(Key::A, ctrl), key(Key::Backspace, Modifiers::NONE)]);
    h.frame(&mut state, &mut shell, vec![]);
    assert_eq!(prefix(&state), "");
    assert!(state.settings_dirty);
    assert!(!has(&h, "mypack"), "no jump back while editing: {:?}", texts(&h));

    h.frame(&mut state, &mut shell, vec![Event::Text("art/bricks".into())]);
    h.frame(&mut state, &mut shell, vec![]);
    assert_eq!(prefix(&state), "art/bricks");
    assert!(!has(&h, "detected"), "{:?}", texts(&h));

    h.frame(&mut state, &mut shell, vec![key(Key::A, ctrl), Event::Text("mypack".into())]);
    assert_eq!(prefix(&state), suggestion);

    h.click(&mut state, &mut shell, "materials/");
    assert!(state.ui.show_settings);
    assert!(h.ctx.memory(|m| m.focused()).is_none());
    assert!(has(&h, "mypack") && has(&h, "detected"), "{:?}", texts(&h));
    assert_eq!(prefix(&state), suggestion);

    h.click(&mut state, &mut shell, "mypack");
    h.frame(&mut state, &mut shell, vec![key(Key::A, ctrl), key(Key::Backspace, Modifiers::NONE)]);
    h.click(&mut state, &mut shell, "materials/");
    assert_eq!(prefix(&state), "");
    assert!(has(&h, "mypack") && has(&h, "detected"), "{:?}", texts(&h));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn extension_points_register_and_render() {
    use hp_app::actions::Category;
    use hp_app::extend::{
        MenuExtension, SettingsPage, StatusContext, StatusSegment, StatusSide, page_order, status_order,
    };

    struct Page;
    impl SettingsPage for Page {
        fn id(&self) -> &'static str {
            "test_page"
        }
        fn label(&self) -> String {
            "Test page".to_owned()
        }
        fn order(&self) -> i32 {
            page_order::EDITING + 10
        }
        fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
            if ui.checkbox(&mut state.settings.edit.paste_offset, "Paste offset").changed() {
                state.settings_dirty = true;
            }
        }
        fn reset(&mut self, state: &mut AppState) {
            state.settings.edit.paste_offset = true;
        }
    }

    struct Segment;
    impl StatusSegment for Segment {
        fn id(&self) -> &'static str {
            "test_segment"
        }
        fn side(&self) -> StatusSide {
            StatusSide::Right
        }
        fn order(&self) -> i32 {
            status_order::CONTENT + 10
        }
        fn ui(&mut self, ui: &mut egui::Ui, _state: &mut AppState, cx: &mut StatusContext) {
            ui.separator();
            if ui.small_button("x").clicked() {
                cx.open_settings = Some("test_page");
            }
        }
    }

    let mut state = state();
    let mut shell = Shell::new();
    shell.add_settings_page(Box::new(Page));
    shell.add_settings_page(Box::new(Page));
    shell.add_status_segment(Box::new(Segment));
    shell.add_menu_extension(MenuExtension::new(Category::View, 40, 30, |ui, _| {
        ui.label("extension");
    }));
    assert_eq!(shell.settings_dialog().page_ids().iter().filter(|id| **id == "test_page").count(), 1);
    let tabs = shell.settings_dialog().tabs();
    let editing = tabs.iter().position(|t| *t == SettingsTab::Editing).unwrap();
    assert_eq!(tabs[editing + 1], SettingsTab::Package("test_page"), "sorted by order: {tabs:?}");
    assert!(shell.status().segment_ids().contains(&"test_segment"));
    shell.open_settings(&mut state, SettingsTab::from_id("test_page"));
    assert_eq!(shell.settings_dialog().tab, SettingsTab::Package("test_page"));
    assert_eq!(SettingsTab::from_id("game"), SettingsTab::Game);
    run_ctx(&mut state, |ctx, state| shell.dialogs(ctx, state));
    run_ui(&mut state, |ui, state| {
        shell.menu_bar(ui, state);
        shell.status_bar(ui, state, Some("hint".into()));
    });
}

#[test]
fn long_menus_stay_on_short_screens() {
    let _g = lang_guard();
    let short = |h: &mut Harness| {
        h.screen = egui::vec2(1366.0, 480.0);
        hp_app::theme::apply(&h.ctx, hp_app::settings::ThemeChoice::Dark, 1.0);
    };
    let mut state = state();
    let mut shell = Shell::new();
    state.settings.language = Lang::En;
    hp_app::i18n::set_lang(Lang::Fr);

    let mut h = Harness::new();
    short(&mut h);
    h.frame(&mut state, &mut shell, vec![]);
    h.click(&mut state, &mut shell, "Map");
    for _ in 0..3 {
        h.frame(&mut state, &mut shell, vec![]);
    }
    let first = h.find("Magnétisme de grille").expect("first section entry");
    let last = h.find("Propriétés de la map").expect("last section entry");
    assert!(last.left() > first.right() - 1.0, "the Map menu is split into columns: {first:?} {last:?}");
    for (text, r) in &h.nodes {
        assert!(r.bottom() <= 480.5 && r.right() <= 1366.5, "{text:?} is off screen: {r:?}");
    }

    let mut h = Harness::new();
    short(&mut h);
    h.frame(&mut state, &mut shell, vec![]);
    h.click(&mut state, &mut shell, "Affichage");
    h.click(&mut state, &mut shell, "Langue");
    h.click(&mut state, &mut shell, "English");
    let shown = hp_app::i18n::lang();
    hp_app::i18n::set_lang(Lang::En);
    assert_eq!(shown, Lang::En, "a submenu in a column layout still works");
}
