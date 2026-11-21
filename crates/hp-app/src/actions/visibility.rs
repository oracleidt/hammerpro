use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { visibility_actions {
    ToolCordon => "tool_cordon", Tools, ToolsSwitch, None, Ready;
    ToggleCordon => "toggle_cordon", View, ViewHide, None, Ready;
    MoveToVisgroup => "move_to_visgroup", View, ViewHide, None, Ready;
    RemoveFromAllVisgroups => "remove_from_all_visgroups", View, ViewHide, None, Ready;
    HiddenToVisgroup => "hidden_to_visgroup", View, ViewHide, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

fn any_hidden(state: &AppState) -> bool {
    let revision = state.doc.revision();
    let cache = &state.runtime.visibility.hidden_cache;
    if let Some((r, hidden)) = cache.get()
        && r == revision
    {
        return hidden;
    }
    let scene = state.doc.scene();
    let hidden = scene.solids().values().any(|n| n.solid.hidden) || scene.entities().values().any(|n| n.entity.hidden);
    cache.set(Some((revision, hidden)));
    hidden
}

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::MoveToVisgroup | Action::RemoveFromAllVisgroups => Some(!state.doc.selection().objects.is_empty()),
        Action::HiddenToVisgroup => Some(any_hidden(state)),
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    match action {
        Action::ToggleCordon => Some(Check::Toggle(state.doc.scene().cordons().active)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    match action {
        Action::ToggleCordon => Some("▣"),
        Action::MoveToVisgroup => Some("📂"),
        Action::HiddenToVisgroup => Some("👁"),
        _ => None,
    }
}

pub(super) const TR: TrTable = &[
    ("action.tool_cordon", "Cordon tool", "Outil cordon"),
    ("action.toggle_cordon", "Use cordons", "Utiliser les cordons"),
    ("action.move_to_visgroup", "Move to visgroup...", "Déplacer vers un visgroup..."),
    ("action.remove_from_all_visgroups", "Remove from all visgroups", "Retirer de tous les visgroups"),
    ("shell.undo.remove_from_all_visgroups", "Remove from all visgroups", "Retrait de tous les visgroups"),
    ("action.hidden_to_visgroup", "Make a visgroup of hidden objects", "Créer un visgroup des objets masqués"),
    ("tool.cordon", "Cordon", "Cordon"),
    ("shell.undo.toggle_cordons", "Toggle cordons", "Activation des cordons"),
    ("shell.undo.new_cordon", "New cordon", "Nouveau cordon"),
    ("shell.undo.delete_cordon", "Delete cordon", "Suppression du cordon"),
    ("shell.undo.rename_cordon", "Rename cordon", "Renommage du cordon"),
    ("shell.undo.toggle_cordon", "Toggle cordon", "Activation du cordon"),
    ("shell.undo.cordon_mode", "Cordon mode", "Mode du cordon"),
    ("shell.undo.resize_cordon", "Resize cordon", "Redimensionnement du cordon"),
    ("shell.undo.set_visgroups", "Set visgroups", "Affectation des visgroups"),
    ("shell.undo.move_to_visgroup", "Move to visgroup", "Déplacement vers un visgroup"),
    ("shell.undo.visgroup_from_hidden_objects", "Visgroup from hidden objects", "Visgroup des objets masqués"),
    ("shell.undo.reorder_visgroup", "Reorder visgroup", "Changement d'ordre du visgroup"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_objects_enable_the_conversion() {
        let mut state = crate::testing::test_state();
        assert_eq!(enabled(&state, Action::HiddenToVisgroup), Some(false));
        state.doc.set_hidden(&[hp_editor::ObjRef::Solid(2)], true);
        assert_eq!(enabled(&state, Action::HiddenToVisgroup), Some(true));
        assert_eq!(enabled(&state, Action::MoveToVisgroup), Some(false));
        state.doc.set_selection(&[hp_editor::ObjRef::Solid(3)]);
        assert_eq!(enabled(&state, Action::MoveToVisgroup), Some(true));
        assert_eq!(enabled(&state, Action::RemoveFromAllVisgroups), Some(true));
        assert_eq!(check(&state, Action::ToggleCordon), Some(Check::Toggle(false)));
        state.doc.set_cordons_enabled(true).expect("on");
        assert_eq!(check(&state, Action::ToggleCordon), Some(Check::Toggle(true)));
        assert!(Action::ToggleCordon.is_ready() && Action::ToolCordon.is_ready());
    }
}
