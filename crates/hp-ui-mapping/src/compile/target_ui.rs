use egui::{RichText, Ui};
use hp_app::{AppState, t, tf};

pub(super) fn target_game_ui(ui: &mut Ui, state: &mut AppState, running: bool) {
    let ctx = ui.ctx().clone();
    let list = hp_app::games::catalogue(Some(&ctx));
    let current = state.settings.game.active_game().to_owned();
    ui.horizontal(|ui| {
        ui.label(t!("target.label")).on_hover_text(t!("target.tip"));
        match &list {
            Some(list) => {
                ui.add_enabled_ui(!running, |ui| {
                    if let Some(id) = hp_app::games::game_combo(ui, "hp_compile_target", &current, list) {
                        hp_app::games::choose_game(state, &id, true);
                    }
                });
            }
            None => {
                hp_app::widgets::spinner(ui);
                ui.label(RichText::new(t!("target.searching")).weak());
            }
        }
    });
    let Some(p) = list.as_ref().and_then(|l| l.iter().find(|p| p.id.eq_ignore_ascii_case(&current))) else { return };
    let f = p.format();
    let mut line = tf!("target.format", f.version, f.static_props.name());
    if !p.is_gmod() {
        line.push_str("  ");
        line.push_str(t!("target.valve_limits"));
    }
    if !p.installed && !p.is_gmod() {
        line.push_str("  ");
        line.push_str(t!("target.not_installed"));
    }
    ui.label(RichText::new(line).weak().small());
}
