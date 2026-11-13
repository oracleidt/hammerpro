use hp_app::actions::{Category, Check};
use hp_app::extend::MenuExtension;
use hp_app::i18n::TrTable;
use hp_app::{Action, AppState, t};

use super::Package;
use crate::app::HammerProApp;

pub(crate) struct Soundscape;

#[derive(Default)]
pub(crate) struct State {
    editor: hp_ui_lab::soundscape::editor::SoundscapeEditor,
    hearing: hp_ui_lab::soundscape::hearing::Hearing,
    last_tick: Option<std::time::Instant>,
}

const HANDLED: &[Action] = &[Action::ToggleSoundscapeView, Action::ToggleSoundscapeHearing];

fn options_menu(ui: &mut egui::Ui, state: &mut AppState) {
    use hp_ui_shell::menu::{menu_entry, submenu};
    submenu(ui, t!("soundscape.menu.options"), |ui| {
        let s = state.settings.lab.soundscape;
        let mut next = s;
        if menu_entry(ui, t!("soundscape.ed.show_areas"), None, s.view, Check::Toggle(s.areas)).clicked() {
            next.areas = !s.areas;
        }
        if menu_entry(ui, t!("soundscape.ed.ambient"), None, true, Check::Toggle(s.ambient_generic)).clicked() {
            next.ambient_generic = !s.ambient_generic;
        }
        if menu_entry(ui, t!("soundscape.ed.mute"), None, true, Check::Toggle(s.mute)).clicked() {
            next.mute = !s.mute;
        }
        ui.horizontal(|ui| {
            ui.label(t!("soundscape.ed.volume"));
            ui.add(egui::Slider::new(&mut next.volume, 0.0..=1.0).fixed_decimals(2));
        });
        if next != s {
            state.settings.lab.soundscape = next;
            state.settings_dirty = true;
        }
    });
}

impl Package for Soundscape {
    fn handled(&self) -> &'static [Action] {
        HANDLED
    }

    fn handle(&self, app: &mut HammerProApp, _ctx: &egui::Context, action: Action) -> bool {
        let s = &mut app.state.settings.lab.soundscape;
        match action {
            Action::ToggleSoundscapeView => s.view = !s.view,
            Action::ToggleSoundscapeHearing => s.hearing = !s.hearing,
            _ => return false,
        }
        app.state.settings_dirty = true;
        true
    }

    fn init(&self, app: &mut HammerProApp) {
        app.viewports.add_layer(Box::new(hp_ui_lab::soundscape::view::SoundscapeLayer::default()));
        app.shell.add_menu_extension(
            MenuExtension::new(Category::View, 20, 13, options_menu).in_submenu("menu.environment"),
        );
    }

    fn map_loaded(&self, app: &mut HammerProApp) {
        app.pkg.soundscape.editor.reset();
        app.state.runtime.lab.soundscape.library_serial += 1;
    }

    fn tick(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        if app.state.settings.lab.soundscape.view {
            hp_ui_lab::soundscape::world::refresh_props(&mut app.state);
        }
        let now = std::time::Instant::now();
        let dt = app.pkg.soundscape.last_tick.map_or(0.0, |t| now.duration_since(t).as_secs_f32());
        app.pkg.soundscape.last_tick = Some(now);
        app.pkg.soundscape.hearing.tick(&mut app.state, dt);
        if app.state.runtime.lab.soundscape.hearing_voices > 0 || app.state.runtime.lab.soundscape.preview.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
    }

    fn dialogs(&self, app: &mut HammerProApp, ctx: &egui::Context) {
        app.pkg.soundscape.editor.show(ctx, &mut app.state);
    }

    fn translations(&self) -> TrTable {
        hp_ui_lab::SOUNDSCAPE_TR
    }
}
