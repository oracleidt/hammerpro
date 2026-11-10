use egui::{RichText, Ui};
use hp_app::extend::{StatusContext, StatusSegment, StatusSide, status_order};
use hp_app::i18n::TrTable;
use hp_app::runtime::GameState;
use hp_app::{AppState, t, tf};

pub(super) fn segments() -> Vec<Box<dyn StatusSegment>> {
    vec![Box::new(GameBadge)]
}

struct GameBadge;

impl StatusSegment for GameBadge {
    fn id(&self) -> &'static str {
        "launch_game"
    }

    fn side(&self) -> StatusSide {
        StatusSide::Right
    }

    fn order(&self) -> i32 {
        status_order::CONTENT + 20
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState, _cx: &mut StatusContext) {
        let launch = &state.runtime.launch;
        let (text, color) = match launch.game {
            GameState::Ready => (t!("shell.launch.ready"), hp_app::theme::accent()),
            GameState::Starting => (t!("shell.launch.starting"), ui.visuals().weak_text_color()),
            GameState::NotRunning | GameState::Unknown => return,
        };
        ui.separator();
        let r = ui.label(RichText::new(format!("🎮 {text}")).small().color(color));
        crate::common::lazy_hover_text(r, || match &launch.pending_map {
            Some(map) => tf!("shell.launch.pending_tip", map),
            None if launch.game == GameState::Ready => t!("shell.launch.ready_tip").to_owned(),
            None => t!("shell.launch.starting_tip").to_owned(),
        });
        if launch.game == GameState::Starting {
            hp_app::widgets::paced_spinner(ui, 10.0);
        }
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.launch.ready", "Game ready", "Jeu prêt"),
    (
        "shell.launch.ready_tip",
        "Garry's Mod is running: running the map loads it there, without restarting the game",
        "Garry's Mod est lancé : lancer la map l'y charge, sans redémarrer le jeu",
    ),
    ("shell.launch.starting", "Game starting", "Jeu en démarrage"),
    ("shell.launch.starting_tip", "Garry's Mod is starting", "Garry's Mod démarre"),
    (
        "shell.launch.pending_tip",
        "Garry's Mod is starting: {0} loads as soon as it is ready",
        "Garry's Mod démarre : {0} se chargera dès qu'il sera prêt",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_badge_draws_in_every_state() {
        let mut state = hp_app::testing::test_state();
        crate::register_translations();
        for game in [GameState::Unknown, GameState::NotRunning, GameState::Starting, GameState::Ready] {
            state.runtime.launch.game = game;
            state.runtime.launch.pending_map = (game == GameState::Starting).then(|| "gm_x".to_owned());
            hp_app::testing::run_ui(&mut state, |ui, state| {
                let mut cx = StatusContext::default();
                GameBadge.ui(ui, state, &mut cx);
            });
        }
        assert!(hp_app::i18n::lint_table(TR).is_empty());
    }
}
