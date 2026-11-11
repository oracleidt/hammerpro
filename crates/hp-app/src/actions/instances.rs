use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::runtime::instances::selected_instances;
use crate::settings::InstanceDisplay;
use crate::state::AppState;

action_list! { instances_actions {
    CreateInstance => "create_instance", Tools, ToolsInstancing, sc(ALT, Key::S), Ready;
    EditInstance => "edit_instance", Tools, ToolsInstancing, None, Ready;
    CollapseInstances => "collapse_instances", Tools, ToolsInstancing, None, Ready;
    CollapseAllInstances => "collapse_all_instances", Tools, ToolsInstancing, None, Ready;
    InstancesHidden => "instances_hidden", Tools, ToolsInstanceDisplay, None, Ready;
    InstancesTinted => "instances_tinted", Tools, ToolsInstanceDisplay, None, Ready;
    InstancesNormal => "instances_normal", Tools, ToolsInstanceDisplay, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::CreateInstance => Some(!state.doc.selection().objects.is_empty()),
        Action::EditInstance | Action::CollapseInstances => Some(!selected_instances(&state.doc).is_empty()),
        Action::CollapseAllInstances => Some(state.runtime.instances.map_has_instances(&state.doc)),
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    let display = state.settings.instances.display;
    Some(match action {
        Action::InstancesHidden => Check::Radio(display == InstanceDisplay::Hidden),
        Action::InstancesTinted => Check::Radio(display == InstanceDisplay::Tinted),
        Action::InstancesNormal => Check::Radio(display == InstanceDisplay::Normal),
        _ => return None,
    })
}

pub(super) fn icon(_action: Action) -> Option<&'static str> {
    None
}

pub(super) const TR: TrTable = &[
    ("action.create_instance", "Create instance from selection...", "Créer une instance depuis la sélection..."),
    ("action.edit_instance", "Edit instance", "Modifier l'instance"),
    ("action.collapse_instances", "Collapse selected instances", "Fusionner les instances sélectionnées"),
    ("action.collapse_all_instances", "Collapse all instances...", "Fusionner toutes les instances..."),
    ("action.instances_hidden", "Instances: hidden", "Instances : masquées"),
    ("action.instances_tinted", "Instances: tinted", "Instances : teintées"),
    ("action.instances_normal", "Instances: normal", "Instances : normales"),
    ("shell.undo.collapse_instances", "Collapse instances", "Fusionner les instances"),
    ("shell.undo.create_instance", "Create instance", "Créer une instance"),
];
