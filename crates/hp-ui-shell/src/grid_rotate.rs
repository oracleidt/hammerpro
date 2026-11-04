use egui::Ui;
use hp_app::i18n::TrTable;
use hp_app::{Action, AppState, Severity, t, tf};

use crate::grids::view_name;

const QUICK_ANGLE: f64 = 45.0;

pub fn rotate_row(ui: &mut Ui, state: &mut AppState) {
    let kind = state.grid_target_view();
    let before = state.grid_angle(kind);
    let mut angle = before;
    let mut target = None;
    ui.horizontal(|ui| {
        ui.label(t!("shell.grid.rotate"));
        let drag = egui::DragValue::new(&mut angle).speed(0.5).range(-360.0..=360.0).max_decimals(4).suffix("°");
        if ui.add(drag).on_hover_text(tf!("shell.grid.rotate_tip", view_name(kind))).changed() {
            target = Some(angle);
        }
        if ui.small_button("0").on_hover_text(t!("shell.grid.rotate_reset")).clicked() {
            target = Some(0.0);
        }
        if ui.small_button("45").clicked() {
            target = Some(QUICK_ANGLE);
        }
    });
    if let Some(a) = target
        && let Err(e) = state.set_grid_angle(kind, a)
    {
        state.notify(Severity::Warning, e);
    }
}

pub fn run(state: &mut AppState, action: Action) {
    let kind = state.grid_target_view();
    let result = match action {
        Action::RotateGrid45 => state.rotate_grid(kind, QUICK_ANGLE),
        _ => match state.align_grid_to_selection(kind) {
            Ok(angle) => {
                state.notify(Severity::Info, tf!("shell.grid.aligned", format!("{angle}"), view_name(kind)));
                Ok(())
            }
            Err(_) => {
                state.notify(Severity::Info, t!("shell.grid.need_align"));
                Ok(())
            }
        },
    };
    if let Err(e) = result {
        state.notify(Severity::Warning, e);
    }
}

pub(crate) const TR: TrTable = &[
    ("shell.grid.rotate", "Rotate grid", "Rotation de la grille"),
    (
        "shell.grid.rotate_tip",
        "Angle of the grid in degrees in the {0} view, any value. The origin axes do not turn",
        "Angle de la grille en degrés dans la vue {0}, toute valeur. Les axes de l'origine ne tournent pas",
    ),
    ("shell.grid.rotate_reset", "Back to 0 degrees", "Retour à 0 degré"),
    ("shell.grid.aligned", "Grid turned to {0} degrees ({1} view)", "Grille tournée à {0} degrés (vue {1})"),
    (
        "shell.grid.need_align",
        "Select a face, a brush or an entity first",
        "Sélectionnez d'abord une face, un brush ou une entité",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use hp_app::ViewKind;

    #[test]
    fn actions_turn_and_align_the_grid() {
        assert!(hp_app::i18n::lint_table(TR).is_empty(), "{:#?}", hp_app::i18n::lint_table(TR));
        let mut s = hp_app::testing::test_state();
        let kind = s.grid_target_view();
        assert_eq!(kind, ViewKind::Top);
        run(&mut s, Action::RotateGrid45);
        assert_eq!(s.grid_angle(kind), 45.0);
        run(&mut s, Action::RotateGrid45);
        assert_eq!(s.grid_angle(kind), 90.0);
        run(&mut s, Action::AlignGridToSelection);
        assert_eq!(s.grid_angle(kind), 90.0, "nothing selected: unchanged");
    }
}
