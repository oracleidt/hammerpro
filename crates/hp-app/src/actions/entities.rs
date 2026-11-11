use crate::actions::{Action, Check};
use crate::i18n::TrTable;
use crate::state::AppState;

action_list! { entities_actions {
    CopyKeyvalues => "copy_keyvalues", Edit, EditProperties, None, Ready;
    PasteKeyvalues => "paste_keyvalues", Edit, EditProperties, None, Ready;
    FindReplaceText => "find_replace_text", Edit, EditFind, sc(CTRL_ALT, Key::R), Ready;
    EditColor => "edit_color", Edit, EditColors, sc(NONE, Key::C), Ready;
    SetDisplayColor => "set_display_color", Edit, EditColors, None, Ready;
    RandomDisplayColors => "random_display_colors", Edit, EditColors, None, Ready;
    ResetDisplayColors => "reset_display_colors", Edit, EditColors, None, Ready;
    EditSoundscapes => "edit_soundscapes", Map, MapProperties, None, Ready;
    ShowSoundBrowser => "show_sound_browser", View, ViewPanels, None, Ready;
    ShowParticleBrowser => "show_particle_browser", View, ViewPanels, None, Ready;
    EntityHelp => "entity_help", Help, HelpDocs, None, Ready;
    SetMaxDistance => "set_max_distance", Edit, EditProperties, None, Ready;
    ToolPath => "tool_path", Tools, ToolsSwitch, None, Ready;
    PathInsertPoint => "path_insert_point", Tools, ToolsPaths, None, Ready;
    PathRemovePoint => "path_remove_point", Tools, ToolsPaths, None, Ready;
}}

pub(super) const REPEATS: &[Action] = &[];

fn entity_selected(state: &AppState) -> bool {
    let scene = state.doc.scene();
    state.doc.selection().objects.iter().any(|o| match o {
        hp_editor::ObjRef::Entity(id) => scene.entity(*id).is_some(),
        hp_editor::ObjRef::Solid(id) => scene.object_of_solid(*id).is_entity(),
    })
}

pub(super) fn enabled(state: &AppState, action: Action) -> Option<bool> {
    match action {
        Action::CopyKeyvalues | Action::EntityHelp | Action::EditColor | Action::SetMaxDistance => {
            Some(entity_selected(state))
        }
        Action::PasteKeyvalues => Some(state.runtime.entities.clipboard.is_some() && entity_selected(state)),
        Action::PathInsertPoint | Action::PathRemovePoint => Some(entity_selected(state)),
        Action::SetDisplayColor | Action::RandomDisplayColors | Action::ResetDisplayColors => {
            Some(!state.doc.selection().objects.is_empty())
        }
        _ => None,
    }
}

pub(super) fn check(state: &AppState, action: Action) -> Option<Check> {
    let b = &state.settings.browser;
    let rt = &state.runtime.entities;
    match action {
        Action::ShowSoundBrowser => Some(Check::Toggle(b.show_sound_browser || rt.sound_window)),
        Action::ShowParticleBrowser => Some(Check::Toggle(b.show_particle_browser || rt.particle_window)),
        _ => None,
    }
}

pub(super) fn icon(action: Action) -> Option<&'static str> {
    Some(match action {
        Action::FindReplaceText => "🔍",
        Action::EditColor | Action::SetDisplayColor => "🎨",
        Action::RandomDisplayColors => "🎲",
        Action::EntityHelp => "ℹ",
        Action::CopyKeyvalues => "🗐",
        Action::PasteKeyvalues => "📋",
        Action::PathInsertPoint => "➕",
        Action::PathRemovePoint => "➖",
        _ => return None,
    })
}

pub(super) const TR: TrTable = &[
    ("action.copy_keyvalues", "Copy keyvalues", "Copier les keyvalues"),
    ("action.paste_keyvalues", "Paste keyvalues", "Coller les keyvalues"),
    ("action.find_replace_text", "Find and replace in entities...", "Rechercher et remplacer dans les entités..."),
    ("action.edit_color", "Edit color...", "Modifier la couleur..."),
    ("action.set_display_color", "Set display color...", "Définir la couleur d'affichage..."),
    ("action.random_display_colors", "Random display colors", "Couleurs d'affichage aléatoires"),
    ("action.reset_display_colors", "Reset display colors", "Couleurs d'affichage par défaut"),
    ("shell.undo.set_display_color", "Set display color", "Définir la couleur d'affichage"),
    ("action.edit_soundscapes", "Soundscapes...", "Soundscapes..."),
    ("action.show_sound_browser", "Sound browser", "Navigateur de sons"),
    ("action.show_particle_browser", "Particle browser", "Navigateur de particules"),
    ("action.entity_help", "Entity help", "Aide de l'entité"),
    ("action.set_max_distance", "Set maximum distance...", "Définir la distance maximale..."),
    ("action.tool_path", "Path tool", "Outil chemin"),
    ("tool.path", "Path", "Chemin"),
    ("action.path_insert_point", "Insert path point after selection", "Insérer un point de chemin après la sélection"),
    ("action.path_remove_point", "Remove path points and relink", "Supprimer des points de chemin et relier"),
    ("shell.undo.paste_keyvalues", "Paste keyvalues", "Coller les keyvalues"),
    ("shell.undo.replace_text", "Replace text", "Remplacer du texte"),
    ("shell.undo.replace_all_uses", "Replace all uses", "Remplacer partout"),
    ("shell.undo.edit_palette", "Edit palette", "Modifier la palette"),
    ("shell.undo.set_maximum_distance", "Set maximum distance", "Définir la distance maximale"),
    ("shell.undo.add_path_point", "Add path point", "Ajout d'un point de chemin"),
    ("shell.undo.create_path", "Create path", "Création d'un chemin"),
    ("shell.undo.close_path_loop", "Close path loop", "Fermeture d'une boucle de chemin"),
    ("shell.undo.link_path_point", "Link path point", "Liaison d'un point de chemin"),
    ("shell.undo.insert_path_point", "Insert path point", "Insertion d'un point de chemin"),
    ("shell.undo.remove_path_point", "Remove path point", "Suppression d'un point de chemin"),
];
